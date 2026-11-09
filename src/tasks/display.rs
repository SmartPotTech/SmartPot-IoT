//! Tarea de la pantalla LCD2004.
//!
//! Corre en el núcleo 1 sobre su propio bus I2C (I2C1, GPIO16/17), así que una
//! pantalla averiada no puede bloquear el bus de los sensores. Se despierta con
//! cada lectura nueva, cada cambio de los actuadores y cada caída del broker, y
//! solo reescribe las filas que cambian; el firmware MicroPython redibuja en
//! los mismos momentos. Si el LCD falta o falla, lo avisa una vez por la
//! consola y lo vuelve a buscar cada 10 s sin afectar al resto del firmware.

use embassy_executor::task;
use embassy_futures::select::{Either4, select4};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::watch::Receiver;
use embassy_time::{Duration, Instant, Timer, with_timeout};
use esp_hal::{Async, i2c::master::I2c};

use crate::config::DEFAULT_CONFIG;
use crate::display_layout::{ICONS, ROWS, Row, render};
use crate::drivers::hd44780::{Hd44780, LCD_ADDRESS};
use crate::events::{ACTUATOR_STATES, BROKER_ONLINE, CA_REJECTED, DISPLAY_SENSORS, WIFI_ONLINE};
use crate::health::{Beat, beat};
use crate::protocol::SensorState;

/// Despertar mínimo: reintento del LCD y respaldo del icono de conexión.
const TICK: Duration = Duration::from_secs(1);
const REPROBE: Duration = Duration::from_secs(10);
const OPERATION_TIMEOUT: Duration = Duration::from_millis(500);
/// Con el broker recién conectado se espera la lectura fresca de la sesión
/// para dibujar una sola vez, como MicroPython; si no llega, se dibuja igual.
const ONLINE_GRACE: Duration = Duration::from_secs(2);

/// Glifos de la ROM A00 del HD44780 (los mismos que `display.py`).
const SPINNER: [char; 3] = ['\u{a1}', '\u{a5}', '\u{db}'];

/// Pasos de 250 ms que el firmware MicroPython dedica al primer intento de Wi-Fi.
const WIFI_WAIT_STEPS: u32 = 120;

type Lcd = Hd44780<I2c<'static, Async>>;

/// Espera el próximo cambio de un `Watch`, o nunca si no hay receptor libre.
async fn next_change<T: Clone, const N: usize>(
    receiver: &mut Option<Receiver<'static, CriticalSectionRawMutex, T, N>>,
) -> T {
    match receiver.as_mut() {
        Some(receiver) => receiver.changed().await,
        None => core::future::pending().await,
    }
}

/// Avisa una vez por pérdida, con el mismo texto que MicroPython.
fn report_unavailable(reported: &mut bool) {
    if !*reported {
        marker!("{}", crate::reason::LCD_UNAVAILABLE);
        *reported = true;
    }
}

#[task]
pub async fn display_task(i2c: I2c<'static, Async>) {
    let mut lcd = Hd44780::new(i2c, LCD_ADDRESS);
    let mut reported = false;
    let mut ready = initialize(&mut lcd).await;
    let mut next_probe = Instant::now() + REPROBE;

    // Pantallas de arranque de MicroPython: solo si el LCD responde al encender.
    if ready {
        ready = boot_screens(&mut lcd).await;
    }
    if !ready {
        report_unavailable(&mut reported);
    }

    let mut shown: [Option<Row>; ROWS] = Default::default();
    let mut renders: u32 = 0;
    let mut ca_notice_done = false;
    let mut ca_rejected = CA_REJECTED.receiver();
    let mut sensors = DISPLAY_SENSORS.receiver();
    let mut actuators = ACTUATOR_STATES.receiver();
    let mut broker = BROKER_ONLINE.receiver();

    let mut reading: Option<SensorState> = None;
    let mut actuator_states = [false; 4];
    let mut online = false;
    let mut online_since: Option<Instant> = None;
    // Lo último que se dibujó: actuadores y conexión.
    let mut drawn: Option<([bool; 4], bool)> = None;

    loop {
        beat(Beat::Display);
        let mut new_reading = false;
        match select4(
            next_change(&mut sensors),
            next_change(&mut actuators),
            next_change(&mut broker),
            Timer::after(TICK),
        )
        .await
        {
            Either4::First(state) => {
                reading = Some(state);
                new_reading = true;
            }
            Either4::Second(states) => actuator_states = states,
            Either4::Third(state) => {
                if state && !online {
                    online_since = Some(Instant::now());
                }
                online = state;
            }
            Either4::Fourth(()) => {}
        }

        if !ready && Instant::now() >= next_probe {
            next_probe = Instant::now() + REPROBE;
            ready = initialize(&mut lcd).await;
            if ready {
                info!("[LCD] Display initialized at 0x{:02X}", LCD_ADDRESS);
                reported = false;
                shown = Default::default();
                drawn = None;
            }
        }

        // Aviso de CA ausente o dañada, una vez, como `build_client` de MicroPython.
        if ready && !ca_notice_done && ca_rejected.as_mut().and_then(|r| r.try_get()) == Some(true)
        {
            ca_notice_done = true;
            ready = show_message(&mut lcd, "Check CA in", "config.json").await;
            sleep_with_beats(Duration::from_secs(5)).await;
            shown = Default::default();
            drawn = None;
        }

        // Como en MicroPython, la pantalla de conexión sigue hasta la primera lectura.
        // Sin pantalla no se dibuja: el aviso ya salió y se reintenta cada 10 s.
        let Some(state) = reading else {
            continue;
        };
        if !ready {
            continue;
        }

        let wanted = (actuator_states, online);
        let waiting_for_session_reading = drawn.is_some_and(|(_, was_online)| !was_online)
            && online
            && online_since.is_some_and(|since| since.elapsed() < ONLINE_GRACE)
            && drawn.is_some_and(|(states, _)| states == actuator_states);
        if !new_reading && (drawn == Some(wanted) || waiting_for_session_reading) {
            continue;
        }

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
                    ready = false;
                    next_probe = Instant::now() + REPROBE;
                    report_unavailable(&mut reported);
                    break;
                }
            }
        }
        // MicroPython cuenta un render por lectura aunque ninguna fila cambie.
        if ready && (changed || new_reading) {
            drawn = Some(wanted);
            renders = renders.wrapping_add(1);
            // Marcador para las capturas automáticas de Wokwi: el render terminó.
            marker!("SMARTPOT-LCD n={}", renders);
        }
    }
}

/// Configura la pantalla y carga los iconos de conexión.
async fn initialize(lcd: &mut Lcd) -> bool {
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
async fn show_message(lcd: &mut Lcd, first: &str, second: &str) -> bool {
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
async fn boot_screens(lcd: &mut Lcd) -> bool {
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
