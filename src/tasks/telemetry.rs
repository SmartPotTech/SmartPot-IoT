//! Agrega las mediciones de sensores y publica una instantánea por intervalo.
//!
//! También lleva el arranque visible por consola, en el mismo orden que
//! `main.py` del firmware MicroPython: espera al Wi-Fi (o avisa de que no
//! hay), al primer intento de SNTP y a la comprobación de la CA, imprime
//! `SMARTPOT-READY` y el primer informe de memoria, y toma la primera lectura
//! sin esperar al broker. Cada sesión MQTT nueva empieza con una lectura fresca.

use core::sync::atomic::Ordering;

use embassy_executor::task;
use embassy_futures::select::{Either3, select3};
use embassy_sync::pubsub::{DynPublisher, DynSubscriber};
use embassy_time::{Duration, Instant, Timer};

use crate::config::SENSOR_INTERVAL_SECS;
use crate::events::telemetry::TelemetryMessage;
use crate::events::{BROKER_ONLINE, DISPLAY_SENSORS, Measurements};
use crate::health::{Beat, beat};
use crate::protocol::telemetry::{Field, FreshState};
use crate::{clock, report};

/// Una lectura caduca si su sensor no respondió en tres periodos de muestreo.
const FRESHNESS_TTL_MS: u64 = SENSOR_INTERVAL_SECS * 3 * 1000;

/// Bienvenida (2 s) más la pantalla «Connecting WiFi» (30 s), como `main.py`.
const WIFI_BOOT_WAIT: Duration = Duration::from_secs(32);

/// Tiempo máximo que el arranque espera al primer intento de SNTP.
const SNTP_BOOT_WAIT: Duration = Duration::from_secs(10);

/// Tiempo que se muestra el aviso de CA ausente o dañada.
#[cfg(feature = "mqtt")]
const CA_NOTICE: Duration = Duration::from_secs(5);

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
    boot_like_micropython().await;
    marker!(
        "SMARTPOT-READY firmware=smartpot-iot/{} reset={}",
        env!("CARGO_PKG_VERSION"),
        crate::report::reset_cause()
    );
    crate::events::READY.store(true, Ordering::Relaxed);
    // Primer informe de memoria justo tras READY; después, el supervisor.
    crate::tasks::supervisor::report_memory();
    // La primera lectura sale en cuanto arranca el bucle, como en MicroPython.
    let mut snapshot_timer = Timer::after(Duration::from_millis(0));
    let mut broker = BROKER_ONLINE.receiver();

    loop {
        beat(Beat::Telemetry);
        let broker_changed = async {
            match broker.as_mut() {
                Some(receiver) => receiver.changed().await,
                None => core::future::pending().await,
            }
        };
        match select3(
            sensors_channel.next_message_pure(),
            &mut snapshot_timer,
            broker_changed,
        )
        .await
        {
            Either3::First(measurement) => {
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

            // Sesión MQTT nueva: lectura fresca al momento, como MicroPython.
            Either3::Third(true) => snapshot_timer = Timer::after(Duration::from_millis(0)),
            Either3::Third(false) => {}

            Either3::Second(_) => {
                let state = sensors.snapshot(Instant::now().as_millis(), FRESHNESS_TTL_MS);
                // Mismo aviso que el DHT22 de MicroPython cuando no responde.
                #[cfg(not(feature = "board-physical"))]
                if state.temperature.is_none() && state.humidity.is_none() {
                    marker!(
                        "Error reading DHT22: {}",
                        report::dht_reason(crate::events::DHT_ERROR.load(Ordering::Relaxed))
                    );
                }
                marker!(
                    "{}",
                    report::reading_table_text(&state, clock::now()).as_str()
                );
                DISPLAY_SENSORS.sender().send(state);
                if !state.is_empty() {
                    telemetry_channel.publish_immediate(TelemetryMessage::SendAll(state));
                }
                snapshot_timer = Timer::after(Duration::from_secs(interval_seconds));
            }
        }
    }
}

/// Espera latiendo para el supervisor hasta que `ready` se cumpla o pase `limit`.
async fn wait_until(limit: Duration, mut ready: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        beat(Beat::Telemetry);
        if ready() {
            return true;
        }
        Timer::after(Duration::from_millis(250)).await;
    }
    ready()
}

/// Arranque de `main.py`: hasta 32 s de Wi-Fi desde el encendido; con Wi-Fi,
/// el primer intento de SNTP; sin él, el aviso «No WiFi». Después la CA.
async fn boot_like_micropython() {
    let mut wifi = crate::events::WIFI_ONLINE.receiver();
    let boot = Instant::from_ticks(0);
    let wifi_wait = WIFI_BOOT_WAIT
        .checked_sub(boot.elapsed())
        .unwrap_or_default();
    let connected = wait_until(wifi_wait, || {
        wifi.as_mut().and_then(|r| r.try_get()) == Some(true)
    })
    .await;
    if connected {
        wait_until(SNTP_BOOT_WAIT, || {
            crate::events::SNTP_ATTEMPTED.load(Ordering::Relaxed)
        })
        .await;
    } else {
        marker!("{}", crate::reason::NO_WIFI);
    }

    // Sin una CA válida la clave del dispositivo no se envía nunca; se avisa
    // ya, con o sin red, como `build_client`.
    #[cfg(feature = "mqtt")]
    if let Some(problem) = crate::tasks::mqtt::ca_problem(&crate::config::DEFAULT_CONFIG) {
        marker!("{}", problem);
        crate::events::CA_REJECTED.sender().send(true);
        wait_until(CA_NOTICE, || false).await;
    }
}
