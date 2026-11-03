//! Manejador de actuadores y banco de salidas con temporización asíncrona y confirmación ACK.

use defmt::info;
use embassy_executor::task;
use embassy_futures::select::{select, Either};
use embassy_sync::pubsub::{DynPublisher, DynSubscriber};
use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::Output;

use crate::events::{AckMessage, Action, Actuators, Command};

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

#[task]
pub async fn catch_commands(
    mut command_channel: DynSubscriber<'static, Command>,
    ack_channel: DynPublisher<'static, AckMessage>,
    mut water: Output<'static>,
    mut lights: Output<'static>,
    mut fan: Output<'static>,
    mut humidifier: Output<'static>,
) {
    let mut water_state = ActuatorRuntime::new();
    let mut lights_state = ActuatorRuntime::new();
    let mut fan_state = ActuatorRuntime::new();
    let mut humidifier_state = ActuatorRuntime::new();

    // Asegurar que todos los pines arranquen en bajo (apagados)
    water.set_low();
    lights.set_low();
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
                info!("[ACTUATOR] Recibido comando: {:?}", cmd);
                let now = Instant::now();

                match cmd.actuator {
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
                    }
                    Actuators::Lights => {
                        match cmd.action {
                            Action::Activate => {
                                lights.set_high();
                                lights_state.active = true;
                                lights_state.until = cmd
                                    .duration_seconds
                                    .map(|s| now + Duration::from_secs(s as u64));
                            }
                            Action::Deactivate => {
                                lights.set_low();
                                lights_state.active = false;
                                lights_state.until = None;
                            }
                        }
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
                    }
                }

                // Generar y publicar la confirmación ACK inmediata
                let ack = AckMessage::success(
                    cmd.id.as_str(),
                    cmd.actuator,
                    cmd.action,
                    cmd.duration_seconds,
                );
                ack_channel.publish_immediate(ack);
            }

            Either::Second(_) => {
                // Tick periódico para verificar expiración de actuadores temporizados
                let now = Instant::now();

                if water_state.active && water_state.until.map_or(false, |u| now >= u) {
                    water.set_low();
                    water_state.active = false;
                    water_state.until = None;
                    info!("[ACTUATOR] Bomba de agua apagada por tiempo");
                }

                if lights_state.active && lights_state.until.map_or(false, |u| now >= u) {
                    lights.set_low();
                    lights_state.active = false;
                    lights_state.until = None;
                    info!("[ACTUATOR] Luz de cultivo apagada por tiempo");
                }

                if fan_state.active && fan_state.until.map_or(false, |u| now >= u) {
                    fan.set_low();
                    fan_state.active = false;
                    fan_state.until = None;
                    info!("[ACTUATOR] Ventilador apagado por tiempo");
                }

                if humidifier_state.active && humidifier_state.until.map_or(false, |u| now >= u) {
                    humidifier.set_low();
                    humidifier_state.active = false;
                    humidifier_state.until = None;
                    info!("[ACTUATOR] Humidificador apagado por tiempo");
                }
            }
        }
    }
}