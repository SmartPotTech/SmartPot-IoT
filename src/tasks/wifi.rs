//! Tareas de conectividad Wi-Fi, pila de red y empaquetado de telemetría.

use defmt::{info, warn};
use embassy_executor::task;
use embassy_net::Runner;
use embassy_time::{Duration, Timer};
use esp_radio::wifi::scan::ScanConfig;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{Config, Interface, WifiController};

#[task]
pub async fn net_task(mut runner: Runner<'static, Interface<'static>>) {
    info!("Starting network stack");
    runner.run().await
}

#[task]
pub async fn wifi_connection_task(
    mut wifi: WifiController<'static>,
    ssid: &'static str,
    password: &'static str,
) -> ! {
    let sta_config = Config::Station(
        StationConfig::default()
            .with_ssid(ssid)
            .with_password(password.into()),
    );
    let scan_config = ScanConfig::default().with_max(10);
    wifi.set_config(&sta_config).unwrap();
    Timer::after(Duration::from_secs(2)).await;

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

        wifi.set_config(&sta_config).unwrap();
        if wifi.connect_async().await.is_err() {
            warn!("[WIFI] Could not connect to Wi-Fi network '{}'", ssid);
        }

        if wifi.is_connected() {
            info!("[WIFI] Wi-Fi connection established and online");
            match wifi.wait_for_disconnect_async().await {
                Ok(info) => {
                    warn!("[WIFI] Wi-Fi disconnected: {}", info.reason);
                }
                Err(e) => {
                    warn!("[WIFI] Error during disconnection: {}", e);
                }
            }
        } else {
            warn!("[WIFI] Wi-Fi offline. Retrying...");
            Timer::after(Duration::from_secs(5)).await;
        }
    }
}
