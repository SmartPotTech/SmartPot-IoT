//! Tareas de conectividad Wi-Fi y de la pila de red.

use embassy_executor::task;
use embassy_net::Runner;
use embassy_time::{Duration, Timer};
use esp_hal::rng::Trng;
use esp_radio::wifi::scan::ScanConfig;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{AuthenticationMethod, Config, Interface, WifiController};

use crate::backoff::Backoff;

/// Primer reintento a los 2 s; nunca se espera más de 60 s.
const RETRY_BASE_MS: u32 = 2_000;
const RETRY_MAX_MS: u32 = 60_000;

#[task]
pub async fn net_task(mut runner: Runner<'static, Interface<'static>>) {
    info!("Starting network stack");
    runner.run().await
}

/// Construye la configuración de estación. Una contraseña vacía indica una red
/// abierta: con el método por defecto (WPA2-Personal) el ESP32 nunca se uniría
/// a ella, como pasaba con `Wokwi-GUEST`.
fn station_config(ssid: &str, password: &str) -> Config {
    let auth_method = if password.is_empty() {
        AuthenticationMethod::None
    } else {
        AuthenticationMethod::Wpa2Personal
    };
    Config::Station(
        StationConfig::default()
            .with_ssid(ssid)
            .with_password(password.into())
            .with_auth_method(auth_method),
    )
}

#[task]
pub async fn wifi_connection_task(
    mut wifi: WifiController<'static>,
    rng: Trng,
    ssid: &'static str,
    password: &'static str,
) -> ! {
    let sta_config = station_config(ssid, password);
    let scan_config = ScanConfig::default().with_max(10);
    // El primer intento sale al arrancar, mientras se ve la bienvenida, como
    // en MicroPython. Los avisos de consola los da `lease_task` al tener IP.
    let mut backoff = Backoff::new(RETRY_BASE_MS, RETRY_MAX_MS);

    loop {
        info!("[WIFI] Connecting to Wi-Fi network: {}", ssid);
        if let Ok(networks) = wifi.scan_async(&scan_config).await {
            for ap in networks {
                info!(
                    "[WIFI SCAN] Detected network: {} (RSSI: {} dBm)",
                    ap.ssid.as_str(),
                    ap.signal_strength
                );
            }
        }

        // Un fallo al aplicar la configuración ya no detiene el firmware: se
        // registra y se reintenta con la misma espera que un fallo de conexión.
        let connected = match wifi.set_config(&sta_config) {
            Ok(()) => match wifi.connect_async().await {
                Ok(_) => wifi.is_connected(),
                Err(e) => {
                    warn!("[WIFI] Could not connect to '{}': {:?}", ssid, e);
                    false
                }
            },
            Err(e) => {
                warn!("[WIFI] Could not apply the station configuration: {:?}", e);
                false
            }
        };

        if connected {
            backoff.reset();
            info!("[WIFI] Wi-Fi connection established");
            match wifi.wait_for_disconnect_async().await {
                Ok(info) => warn!("[WIFI] Wi-Fi disconnected: {:?}", info.reason),
                Err(e) => warn!("[WIFI] Error during disconnection: {:?}", e),
            }
        }

        let delay = backoff.next_delay_ms(rng.random());
        warn!(
            "[WIFI] Wi-Fi offline. Retrying in {} ms (attempt {})",
            delay,
            backoff.attempt()
        );
        Timer::after(Duration::from_millis(delay as u64)).await;
    }
}
