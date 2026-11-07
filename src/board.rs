//! Perfil de placa.
//!
//! Por defecto el firmware usa la misma placa que el firmware MicroPython
//! (`diagram.json` idéntico): DHT22 en GPIO15, potenciómetros de luz, pH, TDS y
//! humedad de suelo en GPIO34/35/32/33, bomba en GPIO19, luz UV en GPIO18,
//! ventilador en GPIO5 y LCD2004 en GPIO16/17. La feature `board-physical`
//! selecciona el prototipo con 3×BME280 tras un TCA9548A, BH1750, curvas reales
//! de TDS/pH y humidificador.

/// Número de actuadores con salida física (bomba, luz UV, ventilador y, en la
/// placa física, humidificador).
#[cfg(not(feature = "board-physical"))]
pub const ACTUATOR_COUNT: usize = 3;
#[cfg(feature = "board-physical")]
pub const ACTUATOR_COUNT: usize = 4;

#[cfg(not(feature = "board-physical"))]
mod pins {
    pub const WATER_PUMP_GPIO: u8 = 19;
    pub const UV_LIGHT_GPIO: u8 = 18;
    pub const FAN_GPIO: u8 = 5;
    pub const HUMIDIFIER_GPIO: Option<u8> = None;
}

#[cfg(feature = "board-physical")]
mod pins {
    pub const WATER_PUMP_GPIO: u8 = 25;
    pub const UV_LIGHT_GPIO: u8 = 26;
    pub const FAN_GPIO: u8 = 27;
    pub const HUMIDIFIER_GPIO: Option<u8> = Some(5);
}

pub use pins::*;

/// Periodo de muestreo por defecto: en la placa de simulación se lee a menudo
/// para que un cambio de potenciómetro se vea en la siguiente lectura, igual
/// que en el firmware MicroPython.
#[cfg(not(feature = "board-physical"))]
pub const DEFAULT_SENSOR_INTERVAL_SECS: u64 = 2;
#[cfg(feature = "board-physical")]
pub const DEFAULT_SENSOR_INTERVAL_SECS: u64 = 10;
