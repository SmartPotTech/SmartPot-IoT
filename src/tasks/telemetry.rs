//! Agrega las mediciones de sensores y publica una instantánea por intervalo.

use embassy_executor::task;
use embassy_futures::select::{Either, select};
use embassy_sync::pubsub::{DynPublisher, DynSubscriber};
use embassy_time::{Duration, Instant, Timer};

use crate::config::SENSOR_INTERVAL_SECS;
use crate::events::telemetry::TelemetryMessage;
use crate::events::{DISPLAY_SENSORS, Measurements};
use crate::health::{Beat, beat};
use crate::protocol::telemetry::{Field, FreshState};
use crate::{clock, report};

/// Una lectura caduca si su sensor no respondió en tres periodos de muestreo.
const FRESHNESS_TTL_MS: u64 = SENSOR_INTERVAL_SECS * 3 * 1000;

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

    let mut sensors = FreshState::default();
    // La primera lectura sale en cuanto hay muestras, como el bucle de MicroPython.
    let mut snapshot_timer = Timer::after(Duration::from_secs(3));

    loop {
        beat(Beat::Telemetry);
        match select(sensors_channel.next_message_pure(), &mut snapshot_timer).await {
            Either::First(measurement) => {
                let now = Instant::now().as_millis();
                match measurement {
                    Measurements::BME280((t, h, p)) => {
                        sensors.set(Field::Temperature, t, now);
                        sensors.set(Field::Humidity, h, now);
                        sensors.set(Field::Atmosphere, p, now);
                    }
                    Measurements::Air((t, h)) => {
                        sensors.set(Field::Temperature, t, now);
                        sensors.set(Field::Humidity, h, now);
                    }
                    Measurements::BH1750(lux) => sensors.set(Field::Brightness, lux, now),
                    Measurements::PH(ph) => sensors.set(Field::Ph, ph, now),
                    Measurements::TDS(tds) => sensors.set(Field::Tds, tds, now),
                    Measurements::SoilMoisture(sm) => sensors.set(Field::SoilMoisture, sm, now),
                }
            }

            Either::Second(_) => {
                let state = sensors.snapshot(Instant::now().as_millis(), FRESHNESS_TTL_MS);
                // Mismo mensaje que el DHT22 de MicroPython cuando no responde.
                #[cfg(not(feature = "board-physical"))]
                if state.temperature.is_none() && state.humidity.is_none() {
                    marker!("Error reading DHT22: [Errno 110] ETIMEDOUT");
                }
                for line in report::reading_table(&state, clock::now()) {
                    marker!("{}", line.as_str());
                }
                DISPLAY_SENSORS.sender().send(state);
                if !state.is_empty() {
                    telemetry_channel.publish_immediate(TelemetryMessage::SendAll(state));
                }
                snapshot_timer = Timer::after(Duration::from_secs(interval_seconds));
            }
        }
    }
}
