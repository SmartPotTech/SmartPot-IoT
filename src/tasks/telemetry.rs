use crate::protocol::SensorState;
use embassy_executor::task;
use embassy_futures::select::{Either, select};
use embassy_sync::pubsub::{DynPublisher, DynSubscriber};
use embassy_time::{Duration, Timer};

use crate::events::Measurements;
use crate::events::telemetry::TelemetryMessage;
use crate::health::{Beat, beat};

#[task]
pub async fn telemetry_task(
    mut sensors_channel: DynSubscriber<'static, Measurements>,
    telemetry_channel: DynPublisher<'static, TelemetryMessage>,
    interval_seconds: u64,
) {
    info!(
        "Telemetry task started with interval of {} s",
        interval_seconds
    );

    let mut sensor_state = SensorState::default();
    let mut snapshot_timer = Timer::after(Duration::from_secs(interval_seconds));

    loop {
        beat(Beat::Telemetry);
        match select(sensors_channel.next_message_pure(), &mut snapshot_timer).await {
            Either::First(m) => match m {
                Measurements::BME280((t, h, p)) => sensor_state.update_bme280(t, h, p),
                Measurements::BH1750(l) => sensor_state.update_lux(l),
                Measurements::PH(p) => sensor_state.update_ph(p),
                Measurements::TDS(t) => sensor_state.update_tds(t),
                Measurements::SoilMoisture(sm) => sensor_state.update_soil_moisture(sm),
            },

            Either::Second(_) => {
                if !sensor_state.is_empty() {
                    let mut payload: heapless::String<255> = heapless::String::new();
                    sensor_state.to_json(&mut payload);
                    info!("[TELEMETRY] Snapshot ready: {}", payload.as_str());
                    telemetry_channel.publish_immediate(TelemetryMessage::SendAll(sensor_state));
                }
                snapshot_timer = Timer::after(Duration::from_secs(interval_seconds));
            }
        }
    }
}
