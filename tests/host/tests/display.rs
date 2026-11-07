//! Pruebas de la maqueta del LCD2004: mismas filas que el firmware MicroPython.

use smartpot_iot_host::display_layout::{COLUMNS, OFFLINE_GLYPH, ONLINE_GLYPH, render};
use smartpot_iot_host::protocol::SensorState;

fn readings() -> SensorState {
    SensorState {
        temperature: Some(23.5),
        humidity: Some(61.0),
        brightness: Some(820.0),
        ph: Some(6.1),
        tds: Some(790.0),
        atmosphere: Some(1013.0),
        soil_moisture: Some(64.0),
    }
}

#[test]
fn rows_match_the_micropython_layout() {
    let rows = render(&readings(), [true, false, true, false], true);
    let mut expected = String::from("T 23.5C H  61%     ");
    expected.push(ONLINE_GLYPH);
    assert_eq!(rows[0].as_str(), expected);
    assert_eq!(rows[1].as_str(), "Light   820 lux     ");
    assert_eq!(rows[2].as_str(), "Soil  64% pH  6.1   ");
    assert_eq!(rows[3].as_str(), "TDS  790ppm [P-F-]  ");
}

#[test]
fn missing_readings_show_dashes_and_the_offline_icon() {
    let rows = render(&SensorState::default(), [false; 4], false);
    let mut expected = String::from("T   --C H  --%     ");
    expected.push(OFFLINE_GLYPH);
    assert_eq!(rows[0].as_str(), expected);
    assert_eq!(rows[3].as_str(), "TDS   --ppm [----]  ");
}

#[test]
fn every_row_fills_exactly_the_screen_width() {
    let extreme = SensorState {
        temperature: Some(-19.99),
        humidity: Some(100.0),
        brightness: Some(199_999.0),
        ph: Some(14.0),
        tds: Some(10_000.0),
        atmosphere: None,
        soil_moisture: Some(100.0),
    };
    for row in render(&extreme, [true; 4], true) {
        assert_eq!(row.chars().count(), COLUMNS, "{row:?}");
    }
}
