#[cfg(feature = "wifi")]
pub mod wifi;

#[cfg(feature = "sensors")]
pub mod sensors;

#[cfg(feature = "actuators")]
pub mod command;

#[cfg(feature = "mqtt")]
pub mod mqtt;

#[cfg(all(
    feature = "sensors",
    any(feature = "mqtt", feature = "http-api", feature = "ble")
))]
pub mod telemetry;
