#![no_std]
extern crate alloc;

pub mod codec;
pub mod config;
pub mod drivers;
pub mod events;
pub mod filter;
pub mod protocol;
pub mod tasks;

pub mod i2c_scanner {
    use defmt::{info, warn};
    use embassy_embedded_hal::shared_bus::asynch::i2c::I2cDevice;
    use embassy_sync::blocking_mutex::raw::NoopRawMutex;
    use embedded_hal_async::i2c::I2c as I2cAsync;
    use esp_hal::{i2c::master::I2c, Async};

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

            for channel in 0..8 {
                let channel_mask = 1 << channel;

                match self.i2c.write(0x70, &[channel_mask]).await {
                    Ok(_) => {
                        info!("Canal {} seleccionado", channel);
                    }
                    Err(_) => {
                        warn!("No se pudo seleccionar el canal {}", channel);
                        continue;
                    }
                }

                for addr in 0x03..=0x77 {
                    if addr == 0x70 {
                        continue;
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
