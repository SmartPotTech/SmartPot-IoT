//! Controlador asíncrono para el sensor de luz ambiental BH1750.
//!
//! Admite medición en modo de alta resolución de una toma (One-Time H-Resolution Mode).

use embassy_time::{Duration, Timer};
use embedded_hal_async::i2c::I2c;

/// Dirección I2C por defecto cuando el pin ADDR está a GND (0x23).
pub const BH1750_DEFAULT_ADDRESS: u8 = 0x23;

/// Dirección I2C alternativa cuando el pin ADDR está a VCC (0x5C).
pub const BH1750_ALT_ADDRESS: u8 = 0x5C;

/// Comando para modo de alta resolución de una sola toma (1 lx de resolución).
const CMD_ONE_TIME_H_RES: u8 = 0x20;

/// Controlador asíncrono del sensor BH1750.
pub struct Bh1750<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C> Bh1750<I2C>
where
    I2C: I2c,
{
    /// Crea una nueva instancia del sensor con la dirección I2C indicada.
    pub const fn new(i2c: I2C, address: u8) -> Self {
        Self { i2c, address }
    }

    /// Realiza una medición de luminosidad en lux de forma asíncrona.
    ///
    /// Envía el comando de medición en alta resolución, espera el tiempo de
    /// conversión de la hoja de datos (típicamente 120 ms, máx 180 ms) sin
    /// bloquear el hilo ejecutor, y lee el resultado de 16 bits.
    pub async fn measure(&mut self) -> Result<f32, I2C::Error> {
        self.i2c.write(self.address, &[CMD_ONE_TIME_H_RES]).await?;
        Timer::after(Duration::from_millis(150)).await;
        let mut buf = [0u8; 2];
        self.i2c.read(self.address, &mut buf).await?;
        let raw = u16::from_be_bytes(buf);
        Ok((raw as f32) / 1.2)
    }
}
