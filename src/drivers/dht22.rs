//! Driver del DHT22 (AM2302) por su bus de un hilo.
//!
//! El pin trabaja en drenador abierto con *pull-up*: el firmware baja la línea
//! para pedir una medición y después solo la lee. Los 40 bits se cronometran con
//! las interrupciones desactivadas (unos 5 ms), igual que hace MicroPython.

use esp_hal::delay::Delay;
use esp_hal::gpio::{DriveMode, Flex, OutputConfig, Pull};
use esp_hal::time::Instant;

use super::dht22_frame::decode;

/// Fallos de lectura del DHT22.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dht22Error {
    /// El sensor no respondió al pulso de inicio (desconectado o sin alimentación).
    NoResponse,
    /// La trama se cortó a mitad de lectura.
    Timeout,
    /// La suma de control no coincide.
    Checksum,
}

pub struct Dht22<'d> {
    pin: Flex<'d>,
}

impl<'d> Dht22<'d> {
    pub fn new(mut pin: Flex<'d>) -> Self {
        pin.apply_output_config(
            &OutputConfig::default()
                .with_drive_mode(DriveMode::OpenDrain)
                .with_pull(Pull::Up),
        );
        pin.set_input_enable(true);
        pin.set_output_enable(true);
        pin.set_high();
        Self { pin }
    }

    /// Lee temperatura (°C) y humedad relativa (%). El sensor necesita al
    /// menos 2 s entre lecturas.
    pub fn read(&mut self) -> Result<(f32, f32), Dht22Error> {
        // Pulso de inicio: al menos 1 ms en bajo.
        self.pin.set_low();
        Delay::new().delay_micros(1_100);

        let bytes = critical_section::with(|_| {
            self.pin.set_high();
            self.read_frame()
        })?;
        decode(bytes).ok_or(Dht22Error::Checksum)
    }

    /// Espera a que la línea deje el nivel `high`; devuelve cuánto duró.
    fn wait_while(&self, high: bool, timeout_us: u64) -> Result<u64, Dht22Error> {
        let start = Instant::now();
        while self.pin.is_high() == high {
            if start.elapsed().as_micros() > timeout_us {
                return Err(Dht22Error::Timeout);
            }
        }
        Ok(start.elapsed().as_micros())
    }

    fn read_frame(&mut self) -> Result<[u8; 5], Dht22Error> {
        // Respuesta: la línea sigue alta unos µs, el sensor la baja 80 µs y la
        // sube otros 80 µs antes del primer bit.
        let no_response = |_| Dht22Error::NoResponse;
        self.wait_while(true, 200).map_err(no_response)?;
        self.wait_while(false, 120).map_err(no_response)?;
        self.wait_while(true, 120).map_err(no_response)?;

        let mut bytes = [0u8; 5];
        for bit in 0..40 {
            // Cada bit empieza con 50 µs en bajo; la duración del nivel alto
            // indica el valor (unos 27 µs = 0, unos 70 µs = 1).
            self.wait_while(false, 100)?;
            let high_us = self.wait_while(true, 120)?;
            if high_us > 45 {
                bytes[bit / 8] |= 0x80 >> (bit % 8);
            }
        }
        Ok(bytes)
    }
}
