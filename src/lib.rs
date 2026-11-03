#![no_std]
extern crate alloc;

pub mod events;
pub mod tasks;

#[cfg(feature = "sensors")]
pub mod drivers;

#[cfg(feature = "sensors")]
pub mod i2c_scanner {
    use defmt::{info, warn};
    use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
    use embassy_sync::blocking_mutex::raw::NoopRawMutex;
    use embedded_hal_async::i2c::I2c as I2cAsync;
    use esp_hal::{Async, i2c::master::I2c};

    pub struct I2CScanner<I2C> {
        i2c: I2C,
    }

    impl<I2C> I2CScanner<I2C>
    where
        I2C: I2cAsync,
    {
        pub async fn new(i2c: I2C) -> Self {
            Self { i2c }
        }

        pub async fn i2c_scan_tca9548a(&mut self) {
            info!("Iniciando escaneo I2C con TCA9548A...");

            // Iterar sobre los 8 canales del TCA
            for channel in 0..8 {
                let channel_mask = 1 << channel;

                // Seleccionar canal en el TCA (0x70)
                match self.i2c.write(0x70, &[channel_mask]).await {
                    Ok(_) => {
                        info!("Canal {} seleccionado", channel);
                    }
                    Err(_) => {
                        warn!("No se pudo seleccionar el canal {}", channel);
                        continue;
                    }
                }

                // (Opcional pero recomendado) pequeño delay
                // embassy_time::Timer::after_millis(5).await;

                // Escanear direcciones en ese canal
                for addr in 0x03..=0x77 {
                    if addr == 0x70 {
                        continue; // evitar el propio TCA
                    }

                    if self.i2c.write(addr, &[]).await.is_ok() {
                        info!("Canal {} → dispositivo en 0x{:02X}", channel, addr);
                    }
                }
            }

            warn!("Escaneo I2C terminado");
        }
    }

    #[embassy_executor::task]
    pub async fn scan_i2c(i2c_bus: I2cDevice<'static, NoopRawMutex, I2c<'static, Async>>) {
        let mut scanner = I2CScanner::new(i2c_bus).await;
        scanner.i2c_scan_tca9548a().await;
    }
}
