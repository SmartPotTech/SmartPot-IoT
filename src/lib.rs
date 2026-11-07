#![no_std]
extern crate alloc;

/// Imprime un marcador de texto plano (`SMARTPOT-...`) por la consola serie.
///
/// Los registros normales viajan en `defmt` binario; estos marcadores son
/// texto legible para el monitor de `espflash`, el simulador Wokwi y sus
/// escenarios automatizados (`wait-serial`).
#[macro_export]
macro_rules! marker {
    ($($arg:tt)*) => {
        ::esp_println::println!($($arg)*)
    };
}

pub mod capabilities;
pub mod clock;
pub mod codec;
pub mod config;
pub mod drivers;
pub mod events;
pub mod filter;
pub mod limits;
pub mod protocol;
pub mod safety;
pub mod tasks;
#[cfg(feature = "mqtt")]
pub mod tls;

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
            info!("Starting I2C scan with TCA9548A...");

            for channel in 0..8 {
                let channel_mask = 1 << channel;

                match self.i2c.write(0x70, &[channel_mask]).await {
                    Ok(_) => {
                        info!("Channel {} selected", channel);
                    }
                    Err(_) => {
                        warn!("Could not select channel {}", channel);
                        continue;
                    }
                }

                for addr in 0x03..=0x77 {
                    if addr == 0x70 {
                        continue;
                    }

                    if self.i2c.write(addr, &[]).await.is_ok() {
                        info!("Channel {} -> device at 0x{:02X}", channel, addr);
                    }
                }
            }

            // Deja el multiplexor sin canal seleccionado para que el bus
            // principal (BH1750) quede accesible al terminar el diagnóstico.
            let _ = self.i2c.write(0x70, &[0x00]).await;

            warn!("I2C scan finished");
        }
    }

    #[embassy_executor::task]
    pub async fn scan_i2c(i2c_bus: I2cDevice<'static, NoopRawMutex, I2c<'static, Async>>) {
        let mut scanner = I2CScanner::new(i2c_bus).await;
        scanner.i2c_scan_tca9548a().await;

        // `I2C_SCAN_DONE` lo esperan las tareas de sensores antes de tocar el
        // bus. La tarea del escáner siempre se lanza con la feature `sensors`.
        crate::events::I2C_SCAN_DONE.sender().send(true);
    }
}
