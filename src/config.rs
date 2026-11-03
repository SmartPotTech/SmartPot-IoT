//! Configuración central del firmware SmartPot ESP32 en Rust.
//!
//! Admite valores predeterminados seguros para desarrollo y entornos locales
//! (como demo-beta con EMQX y la API en Go), así como sobreescrituras en
//! tiempo de compilación mediante variables de entorno (`option_env!`).

pub struct Config {
    pub wifi_ssid: &'static str,
    pub wifi_password: &'static str,
    pub crop_id: &'static str,
    pub device_key: &'static str,
    pub broker_host: &'static str,
    pub broker_port: u16,
    pub use_tls: bool,
    pub telemetry_interval_secs: u64,
    /// Telemetría en el paquete compacto v2 (27 bytes) en vez de JSON; se activa definiendo COMPACT_TELEMETRY al
    /// compilar. La API acepta los dos formatos en el mismo tópico.
    pub compact_telemetry: bool,
}

pub const DEFAULT_CONFIG: Config = Config {
    wifi_ssid: match option_env!("SSID") {
        Some(v) => v,
        None => "Wokwi-GUEST",
    },
    wifi_password: match option_env!("PASSWORD") {
        Some(v) => v,
        None => "",
    },
    crop_id: match option_env!("CROP_ID") {
        Some(v) => v,
        None => "66f5a1000000000000000101",
    },
    device_key: match option_env!("DEVICE_KEY") {
        Some(v) => v,
        None => "demo-lettuce-device-key-2026",
    },
    broker_host: match option_env!("BROKER_HOST") {
        Some(v) => v,
        None => "127.0.0.1",
    },
    broker_port: 1883,
    use_tls: false,
    compact_telemetry: match option_env!("COMPACT_TELEMETRY") {
        Some(_) => true,
        None => false,
    },
    telemetry_interval_secs: 15,
};
