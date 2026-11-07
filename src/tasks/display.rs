//! Tarea de la pantalla LCD2004.
//!
//! Corre en el núcleo 1 sobre su propio bus I2C (I2C1, GPIO16/17), así que una
//! pantalla averiada no puede bloquear el bus de los sensores. Lee el último
//! estado publicado (sin colas ni contrapresión) y solo reescribe las filas que
//! cambian. Si el LCD falta o falla, se vuelve a inicializar cada pocos
//! segundos sin afectar al resto del firmware.

use embassy_executor::task;
use embassy_time::{Duration, Timer, with_timeout};
use esp_hal::{Async, i2c::master::I2c};

use crate::display_layout::{ICONS, ROWS, Row, render};
use crate::drivers::hd44780::{Hd44780, LCD_ADDRESS};
use crate::events::{ACTUATOR_STATES, BROKER_ONLINE, DISPLAY_SENSORS};
use crate::health::{Beat, beat};

const REFRESH: Duration = Duration::from_secs(1);
const RETRY_EVERY: u32 = 10;
const OPERATION_TIMEOUT: Duration = Duration::from_millis(500);

#[task]
pub async fn display_task(i2c: I2c<'static, Async>) {
    let mut lcd = Hd44780::new(i2c, LCD_ADDRESS);
    let mut ready = false;
    let mut shown: [Option<Row>; ROWS] = Default::default();
    let mut renders: u32 = 0;
    let mut cycle: u32 = 0;

    let mut sensors = DISPLAY_SENSORS.receiver();
    let mut actuators = ACTUATOR_STATES.receiver();
    let mut broker = BROKER_ONLINE.receiver();

    loop {
        beat(Beat::Display);

        if !ready && cycle.is_multiple_of(RETRY_EVERY) {
            ready = initialize(&mut lcd).await;
            shown = Default::default();
            if ready {
                info!("[LCD] Display initialized at 0x{:02X}", LCD_ADDRESS);
            }
        }
        cycle = cycle.wrapping_add(1);

        if ready {
            let state = sensors
                .as_mut()
                .and_then(|r| r.try_get())
                .unwrap_or_default();
            let actuator_states = actuators
                .as_mut()
                .and_then(|r| r.try_get())
                .unwrap_or_default();
            let online = broker.as_mut().and_then(|r| r.try_get()).unwrap_or(false);

            let rows = render(
                &state,
                &actuator_states[..crate::board::ACTUATOR_COUNT],
                online,
            );
            let mut changed = false;
            for (index, row) in rows.iter().enumerate() {
                if shown[index].as_ref() == Some(row) {
                    continue;
                }
                match with_timeout(OPERATION_TIMEOUT, lcd.write_row(index, row.as_str())).await {
                    Ok(Ok(())) => {
                        shown[index] = Some(row.clone());
                        changed = true;
                    }
                    _ => {
                        warn!("[LCD] Display not responding; retrying later");
                        ready = false;
                        break;
                    }
                }
            }
            if ready && changed {
                renders = renders.wrapping_add(1);
                // Marcador para las capturas automáticas de Wokwi: el render terminó.
                marker!("SMARTPOT-LCD n={}", renders);
            }
        }

        Timer::after(REFRESH).await;
    }
}

/// Configura la pantalla y carga los iconos de conexión.
async fn initialize(lcd: &mut Hd44780<I2c<'static, Async>>) -> bool {
    let setup = async {
        lcd.init().await?;
        for (location, icon) in ICONS.iter().enumerate() {
            lcd.custom_char(location as u8, icon).await?;
        }
        Ok::<(), esp_hal::i2c::master::Error>(())
    };
    matches!(with_timeout(OPERATION_TIMEOUT * 2, setup).await, Ok(Ok(())))
}
