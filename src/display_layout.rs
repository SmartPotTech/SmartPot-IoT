//! Maqueta del LCD2004 (20 columnas × 4 filas), igual que la del firmware
//! MicroPython para que ambos dispositivos muestren lo mismo.
//!
//! ```text
//! T 23.5C H  61%     ⌁    (⌁ = icono de conexión con el broker)
//! Light   820 lux
//! Soil  64% pH  6.1
//! TDS  790ppm [P-F-]       (bomba, luz UV, ventilador, humidificador)
//! ```
//!
//! Es pura para poder probarla en el host.

use core::fmt::Write;
use heapless::String;

use crate::protocol::SensorState;

/// Columnas y filas del LCD2004.
pub const COLUMNS: usize = 20;
pub const ROWS: usize = 4;

/// Carácter de la CGRAM con la antena (broker en línea) y con la equis.
pub const ONLINE_GLYPH: char = '\u{0}';
pub const OFFLINE_GLYPH: char = '\u{1}';

/// Iconos que se cargan en la CGRAM (posiciones 0 y 1), idénticos a `display.py`.
pub const ICONS: [[u8; 8]; 2] = [
    [
        0b00000, 0b01110, 0b10001, 0b00100, 0b01010, 0b00000, 0b00100, 0b00000,
    ],
    [
        0b00000, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b00000, 0b00000,
    ],
];

/// Una fila del LCD.
pub type Row = String<COLUMNS>;

/// Escribe un número alineado a la derecha, o `--` si no hay lectura.
fn number(row: &mut String<32>, value: Option<f32>, width: usize, decimals: usize) {
    match value {
        Some(value) => {
            let _ = write!(row, "{:>width$.decimals$}", value);
        }
        None => {
            for _ in 0..width.saturating_sub(2) {
                let _ = row.push(' ');
            }
            let _ = row.push_str("--");
        }
    }
}

/// Recorta o rellena hasta el ancho de la pantalla: un texto más largo
/// saltaría a otra fila del HD44780 (la 0 continúa en la 2).
fn fit(text: &str, columns: usize) -> Row {
    let mut row = Row::new();
    for ch in text.chars().take(columns) {
        let _ = row.push(ch);
    }
    while row.len() < columns {
        let _ = row.push(' ');
    }
    row
}

/// Genera las cuatro filas a partir del estado de sensores, de los actuadores
/// (bomba, luz UV, ventilador, humidificador) y de la conexión con el broker.
pub fn render(state: &SensorState, actuators: &[bool], online: bool) -> [Row; ROWS] {
    let mut header: String<32> = String::new();
    let _ = header.push_str("T ");
    number(&mut header, state.temperature, 4, 1);
    let _ = header.push_str("C H ");
    number(&mut header, state.humidity, 3, 0);
    let _ = header.push('%');
    let mut row0 = fit(&header, COLUMNS - 1);
    let _ = row0.push(if online { ONLINE_GLYPH } else { OFFLINE_GLYPH });

    let mut light: String<32> = String::new();
    let _ = light.push_str("Light ");
    number(&mut light, state.brightness, 5, 0);
    let _ = light.push_str(" lux");

    let mut soil: String<32> = String::new();
    let _ = soil.push_str("Soil ");
    number(&mut soil, state.soil_moisture, 3, 0);
    let _ = soil.push_str("% pH ");
    number(&mut soil, state.ph, 4, 1);

    let mut tds: String<32> = String::new();
    let _ = tds.push_str("TDS ");
    number(&mut tds, state.tds, 4, 0);
    let flags = ['P', 'L', 'F', 'H'];
    let _ = tds.push_str("ppm [");
    for (active, flag) in actuators.iter().zip(flags) {
        let _ = tds.push(if *active { flag } else { '-' });
    }
    let _ = tds.push(']');

    [
        row0,
        fit(&light, COLUMNS),
        fit(&soil, COLUMNS),
        fit(&tds, COLUMNS),
    ]
}
