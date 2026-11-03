use defmt::{info, warn};
use embassy_net::Runner;
use embassy_time::{Duration, Timer};
use esp_radio::wifi::scan::ScanConfig;
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config, Interface, Password, Ssid, WifiController,
};

#[embassy_executor::task]
pub async fn net_task(mut runner: Runner<'static, Interface>) {
    info!("Starting network stack");
    runner.run().await
}

#[embassy_executor::task]
pub async fn wifi_connection_task(
    mut wifi: WifiController<'static>,
    ssid: &'static str,
    password: &'static str,
) -> ! {
    let ssid: Ssid = ssid.try_into().expect("SSID too long");
    let password: Password = password.try_into().expect("Password too long");
    let sta_config = Config::Station(
        StationConfig::default()
            .with_ssid(ssid)
            .with_authentication(AuthenticationMethodConfig::Wpa2Personal(password)),
    );
    let scan_config = ScanConfig::default().with_max(10);
    wifi.set_config(&sta_config).unwrap();
    Timer::after(Duration::from_secs(2)).await;

    loop {
        info!("[WIFI] Connecting to wifi...");
        info!("[WIFI] Scanning for Wifi Networks");

        match wifi.scan_async(&scan_config).await {
            Ok(networks) => {
                for ap in networks {
                    info!(
                        "wifi network: {} @ {}db",
                        ap.ssid.as_str(),
                        ap.signal_strength
                    );
                }
            }
            Err(e) => {
                warn!("[WIFI] Scan failed: {}", e);
            }
        }

        // TODO: soportar multiples redes
        wifi.set_config(&sta_config).unwrap();
        if wifi.connect_async().await.is_err() {
            warn!("Unable to connect to wifi {}", ssid);
        }

        if wifi.is_connected() {
            info!("[WIFI] Wifi is online!");
            match wifi.wait_for_disconnect_async().await {
                Ok(info) => {
                    warn!("[Wifi] Wifi disconnected! {}", info.reason);
                }
                Err(e) => {
                    warn!("[WIFI] Error in disconnection to wifi: {}", e);
                }
            }
        } else {
            warn!("[WIFI] Wifi offline");
            Timer::after(Duration::from_secs(5)).await;
        }
    }
}
