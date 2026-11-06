//! Pruebas del documento de capacidades publicado en el broker: JSON válido,
//! esquema 1, siete sensores con rango y los cuatro actuadores instalados.

use serde_json::Value;
use smartpot_iot_host::capabilities::CAPABILITIES_JSON;
use smartpot_iot_host::limits;

/// (clave, unidad, resolución, mínimo, máximo) esperados de cada sensor.
const SENSORS: [(&str, &str, f64, f64, f64); 7] = [
    ("temperature", "C", 0.01, -20.0, 60.0),
    ("humidity", "%", 0.01, 0.0, 100.0),
    ("brightness", "lux", 1.0, 0.0, 200_000.0),
    ("ph", "pH", 0.01, 0.0, 14.0),
    ("tds", "ppm", 1.0, 0.0, 10_000.0),
    ("atmosphere", "hPa", 0.1, 300.0, 1_100.0),
    ("soilMoisture", "%", 0.01, 0.0, 100.0),
];

#[test]
fn capabilities_json_parses_with_the_expected_shape() {
    let document: Value = serde_json::from_str(CAPABILITIES_JSON).expect("valid JSON");
    assert_eq!(document["schema"], 1);
    assert!(document["device"].is_object());
    assert!(
        document["device"]["firmware"]
            .as_str()
            .unwrap()
            .starts_with("smartpot-iot/")
    );
    assert_eq!(document["device"]["hardware"], "esp32-devkit-c-v4");

    let telemetry = &document["device"]["telemetry"];
    assert_eq!(telemetry["compactBytes"], 27);
    let formats: Vec<&str> = telemetry["formats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(formats, ["json-v1", "compact-v2"]);
}

#[test]
fn seven_sensors_declare_unit_resolution_and_range() {
    let document: Value = serde_json::from_str(CAPABILITIES_JSON).unwrap();
    let sensors = document["sensors"].as_array().expect("sensors array");
    assert_eq!(sensors.len(), 7);

    for (sensor, expected) in sensors.iter().zip(SENSORS) {
        let (key, unit, resolution, min, max) = expected;
        assert_eq!(sensor["key"], key);
        assert_eq!(sensor["unit"], unit);
        assert_eq!(sensor["resolution"].as_f64(), Some(resolution));

        let range = sensor["range"].as_array().expect("range array");
        assert_eq!(range.len(), 2);
        assert_eq!(range[0].as_f64(), Some(min));
        assert_eq!(range[1].as_f64(), Some(max));
        assert!(range[0].as_f64() < range[1].as_f64());
    }
}

#[test]
fn sensor_ranges_match_the_shared_api_limits() {
    let document: Value = serde_json::from_str(CAPABILITIES_JSON).unwrap();
    let sensors = document["sensors"].as_array().unwrap();

    let expected_ranges = [
        limits::TEMPERATURE,
        limits::HUMIDITY,
        limits::BRIGHTNESS,
        limits::PH,
        limits::TDS,
        limits::ATMOSPHERE,
        limits::SOIL_MOISTURE,
    ];

    for (sensor, range) in sensors.iter().zip(expected_ranges) {
        assert_eq!(sensor["range"][0].as_f64(), Some(range.min as f64));
        assert_eq!(sensor["range"][1].as_f64(), Some(range.max as f64));
    }
}

#[test]
fn only_the_four_installed_actuators_are_declared() {
    let document: Value = serde_json::from_str(CAPABILITIES_JSON).unwrap();
    let actuators: Vec<&str> = document["actuators"]
        .as_array()
        .expect("actuators array")
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();

    assert_eq!(
        actuators,
        ["WATER_PUMP", "UV_LIGHT", "FAN", "HUMIDIFIER"],
        "doser actuators are not wired on this board"
    );
    assert!(!actuators.contains(&"PH_DOSER"));
    assert!(!actuators.contains(&"NUTRIENT_DOSER"));
}

#[test]
fn document_fits_in_one_retained_mqtt_publish() {
    assert!(CAPABILITIES_JSON.len() < 2_048);
    assert!(CAPABILITIES_JSON.is_ascii());
    // No debe quedar ningún separador colgando que rompa el JSON.
    assert!(!CAPABILITIES_JSON.contains(",}"));
    assert!(!CAPABILITIES_JSON.contains(",]"));
}
