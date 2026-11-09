pub mod supervisor;

#[cfg(feature = "display")]
pub mod display;

#[cfg(feature = "wifi")]
pub mod lease;

#[cfg(feature = "wifi")]
pub mod resolver;

#[cfg(feature = "wifi")]
pub mod time;

#[cfg(feature = "wifi")]
pub mod wifi;

// Prototipo físico (BME280, BH1750, curvas reales).
#[cfg(feature = "board-physical")]
pub mod sensors;

// Placa compartida con el firmware MicroPython (por defecto).
#[cfg(all(feature = "sensors", not(feature = "board-physical")))]
pub mod board_sensors;

#[cfg(feature = "actuators")]
pub mod command;

#[cfg(feature = "mqtt")]
pub mod mqtt;

#[cfg(all(
    feature = "sensors",
    any(feature = "mqtt", feature = "http-api", feature = "ble")
))]
pub mod telemetry;
