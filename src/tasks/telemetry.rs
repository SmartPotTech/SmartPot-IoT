use defmt::info;
use embassy_futures::select::{Either, select};
use embassy_sync::pubsub::{DynPublisher, DynSubscriber};
use embassy_time::{Duration, Timer};

use crate::events::telemetry::TelemetryMessage;
use crate::events::{Measurements, SensorState};

#[embassy_executor::task]
pub async fn telemetry_task(
    mut sensors_channel: DynSubscriber<'static, Measurements>,
    telemetry_channel: DynPublisher<'static, TelemetryMessage>,
) {
    info!("Telemetry task started");

    let mut sensor_state = SensorState::default();
    let mut snapshot_timer = Timer::after(Duration::from_secs(60));

    loop {
        match select(sensors_channel.next_message_pure(), &mut snapshot_timer).await {
            Either::First(m) => {
                sensor_state.update(m);
                telemetry_channel
                    .publish(TelemetryMessage::SendOne(m))
                    .await;
            }

            Either::Second(_) => {
                telemetry_channel.publish_immediate(TelemetryMessage::SendAll(sensor_state));
                sensor_state.clean();
                snapshot_timer = Timer::after(Duration::from_secs(60));
            }
        }
    }
}
