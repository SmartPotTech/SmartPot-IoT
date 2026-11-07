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

use crate::config::DEFAULT_CONFIG;
use crate::display_layout::{ICONS, ROWS, Row, render};
use crate::drivers::hd44780::{Hd44780, LCD_ADDRESS};
use crate::events::{ACTUATOR_STATES, BROKER_ONLINE, CA_REJECTED, DISPLAY_SENSORS, WIFI_ONLINE};
use crate::health::{Beat, beat};

const REFRESH: Duration = Duration::from_secs(1);
const RETRY_EVERY: u32 = 10;
const OPERATION_TIMEOUT: Duration = Duration::from_millis(500);

/// Glifos de la ROM A00 del HD44780 (los mismos que `display.py`).
const SPINNER: [char; 3] = ['\u{a1}', '\u{a5}', '\u{db}'];

/// Pasos de 250 ms que el firmware MicroPython dedica al primer intento de Wi-Fi.
const WIFI_WAIT_STEPS: u32 = 120;

#[task]
pub async fn display_task(i2c: I2c<'static, Async>) {
    let mut lcd = Hd44780::new(i2c, LCD_ADDRESS);
    let mut ready = false;
    let mut shown: [Option<Row>; ROWS] = Default::default();
    let mut renders: u32 = 0;
    let mut boot_done = false;
    let mut ca_notice_done = false;
    let mut ca_rejected = CA_REJECTED.receiver();
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

        // Pantallas de arranque de MicroPython: bienvenida y conexión Wi-Fi.
        if ready && !boot_done {
            ready = boot_screens(&mut lcd).await;
            shown = Default::default();
            boot_done = true;
        }

        // Aviso de CA ausente o dañada, una vez, como `build_client` de MicroPython.
        if ready && !ca_notice_done && ca_rejected.as_mut().and_then(|r| r.try_get()) == Some(true)
        {
            ca_notice_done = true;
            ready = show_message(&mut lcd, "Check CA in", "config.json").await;
            sleep_with_beats(Duration::from_secs(5)).await;
            shown = Default::default();
        }

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

/// Borra la pantalla y centra dos líneas en las filas 1 y 2 (`show_message`).
async fn show_message(lcd: &mut Hd44780<I2c<'static, Async>>, first: &str, second: &str) -> bool {
    let columns = crate::display_layout::COLUMNS;
    let column = |text: &str| (columns.saturating_sub(text.len()) / 2) as u8;
    let draw = async {
        lcd.clear().await?;
        lcd.write_at(1, column(first), first).await?;
        if !second.is_empty() {
            lcd.write_at(2, column(second), second).await?;
        }
        Ok::<(), esp_hal::i2c::master::Error>(())
    };
    matches!(with_timeout(OPERATION_TIMEOUT * 2, draw).await, Ok(Ok(())))
}

/// Espera sin dejar de latir para el supervisor.
async fn sleep_with_beats(duration: Duration) {
    for _ in 0..duration.as_millis() / 250 {
        beat(Beat::Display);
        Timer::after(Duration::from_millis(250)).await;
    }
}

/// «Welcome to SmartPot ESP32» 2 s y «Connecting WiFi» con *spinner* hasta que
/// el Wi-Fi conecta o pasan 30 s, igual que `main.py`.
async fn boot_screens(lcd: &mut Hd44780<I2c<'static, Async>>) -> bool {
    if !show_message(lcd, "Welcome to", "SmartPot ESP32").await {
        return false;
    }
    sleep_with_beats(Duration::from_secs(2)).await;
    if !show_message(lcd, "Connecting WiFi", DEFAULT_CONFIG.wifi_ssid).await {
        return false;
    }
    let mut wifi = WIFI_ONLINE.receiver();
    let last_column = (crate::display_layout::COLUMNS - 1) as u8;
    let mut glyph = [0u8; 4];
    for step in 0..WIFI_WAIT_STEPS {
        beat(Beat::Display);
        if wifi.as_mut().and_then(|r| r.try_get()) == Some(true) {
            break;
        }
        let symbol = SPINNER[step as usize % SPINNER.len()].encode_utf8(&mut glyph);
        if !matches!(
            with_timeout(OPERATION_TIMEOUT, lcd.write_at(3, last_column, symbol)).await,
            Ok(Ok(()))
        ) {
            return false;
        }
        Timer::after(Duration::from_millis(250)).await;
    }
    true
}
