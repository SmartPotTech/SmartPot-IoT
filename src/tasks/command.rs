//! Manejador de actuadores y banco de salidas con temporización asíncrona y confirmación ACK.

use defmt::{info, warn};
use embassy_executor::task;
use embassy_futures::select::{Either, select};
use embassy_sync::pubsub::{DynPublisher, DynSubscriber};
use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::Output;

use crate::events::{AckMessage, Action, Actuators, Command};

/// Número de ACK recientes que se conservan para detectar comandos duplicados.
const ACK_HISTORY_LEN: usize = 16;

struct ActuatorRuntime {
    active: bool,
    until: Option<Instant>,
}

impl ActuatorRuntime {
    const fn new() -> Self {
        Self {
            active: false,
            until: None,
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

#[task]
pub async fn catch_commands(
    mut command_channel: DynSubscriber<'static, Command>,
    ack_channel: DynPublisher<'static, AckMessage>,
    mut water: Output<'static>,
    mut uv_light: Output<'static>,
    mut fan: Output<'static>,
    mut humidifier: Output<'static>,
) {
    let mut water_state = ActuatorRuntime::new();
    let mut uv_light_state = ActuatorRuntime::new();
    let mut fan_state = ActuatorRuntime::new();
    let mut humidifier_state = ActuatorRuntime::new();
    let mut history = AckHistory::new();

    // Asegurar que todos los pines arranquen en bajo (apagados)
    water.set_low();
    uv_light.set_low();
    fan.set_low();
    humidifier.set_low();

    loop {
        match select(
            command_channel.next_message_pure(),
            Timer::after(Duration::from_millis(250)),
        )
        .await
        {
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
                    ack_channel.publish_immediate(previous.clone());
                    continue;
                }

                let now = Instant::now();
                let ack = match cmd.actuator {
                    Actuators::WaterPump => {
                        match cmd.action {
                            Action::Activate => {
                                water.set_high();
                                water_state.active = true;
                                water_state.until = cmd
                                    .duration_seconds
                                    .map(|s| now + Duration::from_secs(s as u64));
                            }
                            Action::Deactivate => {
                                water.set_low();
                                water_state.active = false;
                                water_state.until = None;
                            }
                        }
                        AckMessage::success(
                            cmd.id.as_str(),
                            cmd.actuator,
                            cmd.action,
                            cmd.duration_seconds,
                        )
                    }
                    Actuators::UvLight => {
                        match cmd.action {
                            Action::Activate => {
                                uv_light.set_high();
                                uv_light_state.active = true;
                                uv_light_state.until = cmd
                                    .duration_seconds
                                    .map(|s| now + Duration::from_secs(s as u64));
                            }
                            Action::Deactivate => {
                                uv_light.set_low();
                                uv_light_state.active = false;
                                uv_light_state.until = None;
                            }
                        }
                        AckMessage::success(
                            cmd.id.as_str(),
                            cmd.actuator,
                            cmd.action,
                            cmd.duration_seconds,
                        )
                    }
                    Actuators::Fan => {
                        match cmd.action {
                            Action::Activate => {
                                fan.set_high();
                                fan_state.active = true;
                                fan_state.until = cmd
                                    .duration_seconds
                                    .map(|s| now + Duration::from_secs(s as u64));
                            }
                            Action::Deactivate => {
                                fan.set_low();
                                fan_state.active = false;
                                fan_state.until = None;
                            }
                        }
                        AckMessage::success(
                            cmd.id.as_str(),
                            cmd.actuator,
                            cmd.action,
                            cmd.duration_seconds,
                        )
                    }
                    Actuators::Humidifier => {
                        match cmd.action {
                            Action::Activate => {
                                humidifier.set_high();
                                humidifier_state.active = true;
                                humidifier_state.until = cmd
                                    .duration_seconds
                                    .map(|s| now + Duration::from_secs(s as u64));
                            }
                            Action::Deactivate => {
                                humidifier.set_low();
                                humidifier_state.active = false;
                                humidifier_state.until = None;
                            }
                        }
                        AckMessage::success(
                            cmd.id.as_str(),
                            cmd.actuator,
                            cmd.action,
                            cmd.duration_seconds,
                        )
                    }
                    // Los dosificadores existen en el contrato, pero esta placa
                    // no tiene salida GPIO para ellos: se rechazan sin accionar
                    // ningún pin ni arrancar temporizadores.
                    Actuators::PhDoser | Actuators::NutrientDoser => {
                        let message = cmd
                            .actuator
                            .unavailable_message()
                            .unwrap_or("This device does not have this doser installed");
                        warn!(
                            "[ACTUATOR] Rejected command {}: {}",
                            cmd.id.as_str(),
                            message
                        );
                        AckMessage::failure(cmd.id.as_str(), message)
                    }
                };

                history.push(ack.clone());
                ack_channel.publish_immediate(ack);
            }

            Either::Second(_) => {
                // Tick periódico para verificar expiración de actuadores temporizados
                let now = Instant::now();

                if water_state.active && water_state.until.is_some_and(|u| now >= u) {
                    water.set_low();
                    water_state.active = false;
                    water_state.until = None;
                    info!("[ACTUATOR] Water pump turned off after timer elapsed");
                }

                if uv_light_state.active && uv_light_state.until.is_some_and(|u| now >= u) {
                    uv_light.set_low();
                    uv_light_state.active = false;
                    uv_light_state.until = None;
                    info!("[ACTUATOR] UV light turned off after timer elapsed");
                }

                if fan_state.active && fan_state.until.is_some_and(|u| now >= u) {
                    fan.set_low();
                    fan_state.active = false;
                    fan_state.until = None;
                    info!("[ACTUATOR] Fan turned off after timer elapsed");
                }

                if humidifier_state.active && humidifier_state.until.is_some_and(|u| now >= u) {
                    humidifier.set_low();
                    humidifier_state.active = false;
                    humidifier_state.until = None;
                    info!("[ACTUATOR] Humidifier turned off after timer elapsed");
                }
            }
        }
    }
}
