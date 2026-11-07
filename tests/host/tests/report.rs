//! La consola del firmware Rust debe imprimir exactamente lo mismo que el
//! firmware MicroPython. Las líneas esperadas salen de ejecutar
//! `utils.print_table` y `time.gmtime` del firmware MicroPython.

use smartpot_iot_host::protocol::SensorState;
use smartpot_iot_host::report::{localtime_tuple, reading_table};

#[test]
fn reading_table_matches_micropython_print_table() {
    let state = SensorState {
        temperature: Some(24.0),
        humidity: Some(40.0),
        brightness: Some(1000.0),
        ph: Some(7.0),
        tds: Some(1500.0),
        atmosphere: None,
        soil_moisture: None,
    };
    let lines: Vec<String> = reading_table(&state, Some(1_794_046_494))
        .iter()
        .map(|line| line.as_str().to_owned())
        .collect();
    assert_eq!(
        lines,
        [
            "+----------------+----------------+",
            "| Reading 2026-11-07 10:14:54 UTC |",
            "+----------------+----------------+",
            "| Temperature    | 24.00 °C       |",
            "| Air humidity   | 40.00 %        |",
            "| Light          | 1000.00 lux    |",
            "| pH             | 7.00           |",
            "| TDS            | 1500.00 ppm    |",
            "| Soil moisture  | --             |",
            "+----------------+----------------+",
        ]
    );
}

#[test]
fn an_unsynchronized_clock_is_reported_like_micropython() {
    let table = reading_table(&SensorState::default(), None);
    assert_eq!(table[1].as_str(), "| Reading unsynchronized clock    |");
    assert_eq!(table[3].as_str(), "| Temperature    | --             |");
}

#[test]
fn clock_tuple_matches_time_localtime() {
    assert_eq!(
        localtime_tuple(1_794_046_494).as_str(),
        "(2026, 11, 7, 10, 14, 54, 5, 311)"
    );
    // 29 de febrero de 2028 (bisiesto): día 60 del año.
    assert_eq!(
        localtime_tuple(1_835_395_200).as_str(),
        "(2028, 2, 29, 0, 0, 0, 1, 60)"
    );
}
