//! Manejador de actuadores con temporización asíncrona, confirmación ACK y
//! política de seguridad (tope de duración y failsafe sin broker).

use embassy_executor::task;
use embassy_futures::select::{Either, select};
use embassy_sync::pubsub::{DynPublisher, DynSubscriber};
use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::Output;

use crate::events::{AckMessage, AckStatus, Action, Actuators, BROKER_ONLINE, Command};
use crate::health::{Beat, beat};
use crate::safety;

/// Número de ACK recientes que se conservan para detectar comandos duplicados.
const ACK_HISTORY_LEN: usize = 16;

/// Periodo con el que se revisan temporizadores y failsafe.
const TICK: Duration = Duration::from_millis(250);

/// Salida física de un actuador con su estado de temporización.
struct Channel {
    kind: Actuators,
    pin: Output<'static>,
    /// Instante en que se encendió; `None` si está apagado.
    on_since: Option<Instant>,
    /// Instante de apagado automático; `None` si no tiene temporizador.
    until: Option<Instant>,
}

impl Channel {
    fn new(kind: Actuators, mut pin: Output<'static>) -> Self {
        pin.set_low();
        Self {
            kind,
            pin,
            on_since: None,
            until: None,
        }
    }

    fn turn_on(&mut self, now: Instant, duration: Option<u32>) {
        self.pin.set_high();
        self.on_since = Some(now);
        self.until = duration.map(|s| now + Duration::from_secs(s as u64));
        match duration {
            Some(seconds) => marker!("SMARTPOT-ACT {} ON {}s", self.kind.key(), seconds),
            None => marker!("SMARTPOT-ACT {} ON", self.kind.key()),
        }
    }

    fn turn_off(&mut self, reason: &str) {
        let was_on = self.on_since.is_some();
        self.pin.set_low();
        self.on_since = None;
        self.until = None;
        if was_on {
            marker!("SMARTPOT-ACT {} OFF {}", self.kind.key(), reason);
        }
    }
}

/// Historial circular local al núcleo 1 con las últimas confirmaciones emitidas.
///
/// Guarda el ACK completo (no solo el identificador) para poder repetir tanto
/// éxitos como fallos cuando el broker reentrega un comando QoS 1.
struct AckHistory {
    entries: [Option<AckMessage>; ACK_HISTORY_LEN],
    next: usize,
}

impl AckHistory {
    const fn new() -> Self {
        Self {
            entries: [const { None }; ACK_HISTORY_LEN],
            next: 0,
        }
    }

    fn find(&self, id: &str) -> Option<&AckMessage> {
        self.entries
            .iter()
            .flatten()
            .find(|ack| ack.id.as_str() == id)
    }

    fn push(&mut self, ack: AckMessage) {
        self.entries[self.next] = Some(ack);
        self.next = (self.next + 1) % ACK_HISTORY_LEN;
    }
}

/// Publica el ACK hacia la tarea MQTT y deja constancia en la consola serie.
fn publish_ack(ack_channel: &DynPublisher<'static, AckMessage>, ack: AckMessage) {
    let status = match ack.status {
        AckStatus::Executed => "EXECUTED",
        AckStatus::Failed => "FAILED",
    };
    marker!(
        "SMARTPOT-ACK {} {} {}",
        ack.id.as_str(),
        status,
        ack.message.as_str()
    );
    ack_channel.publish_immediate(ack);
}

/// Ejecuta un comando validado y devuelve su confirmación.
fn execute(channels: &mut [Channel], cmd: &Command, now: Instant) -> AckMessage {
    let Some(channel) = channels.iter_mut().find(|c| c.kind == cmd.actuator) else {
        // Los dosificadores existen en el contrato, pero esta placa no tiene
        // salida GPIO para ellos: se rechazan sin accionar ningún pin.
        let message = cmd
            .actuator
            .unavailable_message()
            .unwrap_or("This device does not have this actuator installed");
        warn!(
            "[ACTUATOR] Rejected command {}: {}",
            cmd.id.as_str(),
            message
        );
        return AckMessage::failure(cmd.id.as_str(), message);
    };

    match cmd.action {
        Action::Activate => {
            let duration = safety::effective_duration(cmd.actuator, cmd.duration_seconds);
            channel.turn_on(now, duration);
            let mut ack = AckMessage::success(cmd.id.as_str(), cmd.actuator, cmd.action, duration);
            if cmd.duration_seconds.is_none() && duration.is_some() {
                // El tope de seguridad se aplicó en lugar de un encendido indefinido.
                let _ = ack.message.push_str(" (safety limit)");
            }
            ack
        }
        Action::Deactivate => {
            channel.turn_off("command");
            AckMessage::success(cmd.id.as_str(), cmd.actuator, cmd.action, None)
        }
    }
}

#[task]
pub async fn catch_commands(
    mut command_channel: DynSubscriber<'static, Command>,
    ack_channel: DynPublisher<'static, AckMessage>,
    water: Output<'static>,
    uv_light: Output<'static>,
    fan: Output<'static>,
    humidifier: Output<'static>,
) {
    // Todas las salidas arrancan en bajo (apagadas) al construir cada canal.
    let mut channels = [
        Channel::new(Actuators::WaterPump, water),
        Channel::new(Actuators::UvLight, uv_light),
        Channel::new(Actuators::Fan, fan),
        Channel::new(Actuators::Humidifier, humidifier),
    ];
    let mut history = AckHistory::new();

    let mut broker = BROKER_ONLINE.receiver();
    // Al arrancar no hay broker: el failsafe cuenta desde el inicio.
    let mut offline_since: Option<Instant> = Some(Instant::now());

    loop {
        beat(Beat::Command);
        #[cfg(feature = "sim")]
        while crate::sim::hooks::core1_should_hang() {
            core::hint::spin_loop();
        }
        match select(command_channel.next_message_pure(), Timer::after(TICK)).await {
            Either::First(cmd) => {
                info!("[ACTUATOR] Received command: {:?}", cmd);

                // El broker puede reentregar un comando QoS 1 tras un corte. En
                // ese caso no se vuelve a accionar el GPIO ni se reinicia el
                // temporizador: solo se repite la confirmación ya emitida.
                if let Some(previous) = history.find(cmd.id.as_str()) {
                    warn!(
                        "[ACTUATOR] Duplicate command id {} received; re-publishing stored ACK",
                        cmd.id.as_str()
                    );
                    publish_ack(&ack_channel, previous.clone());
                    continue;
                }

                let ack = execute(&mut channels, &cmd, Instant::now());
                history.push(ack.clone());
                publish_ack(&ack_channel, ack);
            }

            Either::Second(_) => {
                let now = Instant::now();

                if let Some(online) = broker.as_mut().and_then(|r| r.try_get()) {
                    match (online, offline_since) {
                        (true, _) => offline_since = None,
                        (false, None) => offline_since = Some(now),
                        (false, Some(_)) => {}
                    }
                }

                for channel in channels.iter_mut() {
                    let Some(on_since) = channel.on_since else {
                        continue;
                    };

                    if channel.until.is_some_and(|until| now >= until) {
                        info!(
                            "[ACTUATOR] {} turned off after timer elapsed",
                            channel.kind.name()
                        );
                        channel.turn_off("timer");
                        continue;
                    }

                    let offline_for = offline_since.map(|since| (now - since).as_millis());
                    if safety::failsafe_due(channel.kind, offline_for, (now - on_since).as_millis())
                    {
                        warn!(
                            "[ACTUATOR] {} turned off: broker unreachable for {} s",
                            channel.kind.name(),
                            safety::OFFLINE_CUTOFF_SECONDS
                        );
                        channel.turn_off("failsafe");
                    }
                }
            }
        }
    }
}
