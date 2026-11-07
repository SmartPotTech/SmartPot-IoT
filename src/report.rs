//! Textos de consola idénticos a los del firmware MicroPython.
//!
//! La tabla de lecturas reproduce `utils.print_table` y la fecha reproduce
//! `time.gmtime()`/`time.localtime()` de MicroPython (UTC). Es puro para
//! poder probarlo en el host; la impresión la hacen las tareas.

use core::fmt::Write;
use heapless::{String, Vec};

use crate::protocol::SensorState;

/// Una línea de la tabla de lecturas.
pub type Line = String<64>;

/// Etiquetas y unidades en el mismo orden que `LABELS` de `utils.py`.
const LABELS: [(&str, &str); 6] = [
    ("Temperature", "°C"),
    ("Air humidity", "%"),
    ("Light", "lux"),
    ("pH", ""),
    ("TDS", "ppm"),
    ("Soil moisture", "%"),
];

/// Fecha civil UTC: (año, mes, día, hora, minuto, segundo, día de la semana
/// con lunes = 0, día del año desde 1), como la tupla de MicroPython.
pub fn civil(epoch: u64) -> (i64, u32, u32, u32, u32, u32, u32, u32) {
    let days = (epoch / 86_400) as i64;
    let seconds = (epoch % 86_400) as u32;
    // Algoritmo de días civiles de Howard Hinnant.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    if month <= 2 {
        year += 1;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    const BEFORE: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let yearday = BEFORE[(month - 1) as usize] + day + u32::from(leap && month > 2);
    // El 1 de enero de 1970 fue jueves (3 con lunes = 0).
    let weekday = (days + 3).rem_euclid(7) as u32;
    (
        year,
        month,
        day,
        seconds / 3_600,
        seconds % 3_600 / 60,
        seconds % 60,
        weekday,
        yearday,
    )
}

/// Tupla de `time.localtime()` tal como la imprime MicroPython.
pub fn localtime_tuple(epoch: u64) -> Line {
    let (y, mo, d, h, mi, s, wd, yd) = civil(epoch);
    let mut line = Line::new();
    let _ = write!(line, "({y}, {mo}, {d}, {h}, {mi}, {s}, {wd}, {yd})");
    line
}

/// Marca de tiempo de la tabla, o el aviso de reloj sin sincronizar.
fn timestamp(epoch: Option<u64>) -> Line {
    let mut text = Line::new();
    match epoch {
        Some(epoch) => {
            let (y, mo, d, h, mi, s, _, _) = civil(epoch);
            let _ = write!(text, "{y}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02} UTC");
        }
        None => {
            let _ = text.push_str("unsynchronized clock");
        }
    }
    text
}

fn row(label: &str, value: Option<f32>, unit: &str) -> Line {
    let mut cell: String<32> = String::new();
    match value {
        Some(value) => {
            let _ = write!(cell, "{value:.2} {unit}");
        }
        None => {
            let _ = cell.push_str("--");
        }
    }
    let mut line = Line::new();
    let _ = write!(line, "| {:<14} | {:<14} |", label, cell.trim_end());
    line
}

/// Tabla de lecturas de `utils.print_table`, línea a línea.
pub fn reading_table(state: &SensorState, epoch: Option<u64>) -> Vec<Line, 10> {
    let separator = {
        let mut line = Line::new();
        let _ = write!(line, "+{:-<16}+{:-<16}+", "", "");
        line
    };
    let mut header_text: String<48> = String::new();
    let _ = write!(header_text, "Reading {}", timestamp(epoch));
    let mut header = Line::new();
    let _ = write!(header, "| {:<31} |", header_text);

    let values = [
        state.temperature,
        state.humidity,
        state.brightness,
        state.ph,
        state.tds,
        state.soil_moisture,
    ];

    let mut lines = Vec::new();
    let _ = lines.push(separator.clone());
    let _ = lines.push(header);
    let _ = lines.push(separator.clone());
    for ((label, unit), value) in LABELS.iter().zip(values) {
        let _ = lines.push(row(label, value, unit));
    }
    let _ = lines.push(separator);
    lines
}
