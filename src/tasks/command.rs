use defmt::info;
use embassy_executor::task;
use embassy_sync::pubsub::DynSubscriber;
use esp_hal::gpio::Output;

use crate::events::{Actuators, Command};

#[task]
pub async fn catch_commands(
    mut command_channel: DynSubscriber<'static, Command>,
    mut w: Output<'static>,
    mut l: Output<'static>,
    mut h: Output<'static>,
) {
    loop {
        let command = command_channel.next_message_pure().await;

        info!("Command received: {:?}", command);
        match command {
            Command::Activate(actuator) => match actuator {
                Actuators::WaterPump => w.set_high(),
                Actuators::Lights => l.set_high(),
                Actuators::Humidifier => h.set_high(),
            },

            Command::Disable(actuator) => match actuator {
                Actuators::WaterPump => w.set_low(),
                Actuators::Lights => l.set_low(),
                Actuators::Humidifier => h.set_low(),
            },

            #[cfg(feature = "sensors")]
            Command::Threshold(_) => {
                // TODO
            }
        }
    }
}
