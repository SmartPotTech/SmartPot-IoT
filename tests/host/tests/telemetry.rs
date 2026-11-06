//! Pruebas del JSON de telemetría v1: los valores no finitos o fuera de rango
//! se omiten sin invalidar el resto del paquete.

use smartpot_iot_host::protocol::SensorState;

fn render(state: &SensorState) -> heapless::String<512> {
    let mut buffer = heapless::String::new();
    state.to_json(&mut buffer);
    buffer
}

#[test]
fn out_of_range_and_non_finite_fields_are_omitted() {
    let state = SensorState {
        temperature: Some(f32::NAN),
        humidity: Some(150.0),
        brightness: Some(-1.0),
        ph: Some(14.5),
        tds: Some(10_500.0),
        atmosphere: Some(1_200.0),
        soil_moisture: Some(f32::INFINITY),
    };
    assert_eq!(render(&state).as_str(), "{}");
}

#[test]
fn valid_fields_are_kept_and_only_the_invalid_one_is_dropped() {
    let state = SensorState {
        temperature: Some(21.5),
        humidity: Some(101.0),
        brightness: Some(12_345.0),
        ph: Some(6.4),
        tds: None,
        atmosphere: Some(1_013.2),
        soil_moisture: Some(48.0),
    };
    let json = render(&state);
    assert!(json.contains("\"temperature\":21.5"));
    assert!(!json.contains("humidity"));
    assert!(json.contains("\"brightness\":12345"));
    assert!(json.contains("\"ph\":6.40"));
    assert!(!json.contains("\"tds\""));
    assert!(json.contains("\"atmosphere\":1013.2"));
    assert!(json.contains("\"soilMoisture\":48.0"));
}

#[test]
fn the_complete_packet_has_the_contract_keys_and_format() {
    let state = SensorState {
        temperature: Some(21.5),
        humidity: Some(40.0),
        brightness: Some(12_345.0),
        ph: Some(6.4),
        tds: Some(800.0),
        atmosphere: Some(1_013.2),
        soil_moisture: Some(48.0),
    };
    assert_eq!(
        render(&state).as_str(),
        concat!(
            "{",
            "\"temperature\":21.5,",
            "\"humidity\":40.0,",
            "\"brightness\":12345,",
            "\"ph\":6.40,",
            "\"tds\":800,",
            "\"atmosphere\":1013.2,",
            "\"soilMoisture\":48.0",
            "}"
        )
    );
}

#[test]
fn exact_api_boundaries_are_kept() {
    let state = SensorState {
        temperature: Some(60.0),
        humidity: Some(0.0),
        brightness: Some(200_000.0),
        ph: Some(14.0),
        tds: Some(10_000.0),
        atmosphere: Some(300.0),
        soil_moisture: Some(100.0),
    };
    let json = render(&state);
    assert!(json.contains("\"temperature\":60.0"));
    assert!(json.contains("\"humidity\":0.0"));
    assert!(json.contains("\"brightness\":200000"));
    assert!(json.contains("\"ph\":14.00"));
    assert!(json.contains("\"tds\":10000"));
    assert!(json.contains("\"atmosphere\":300.0"));
    assert!(json.contains("\"soilMoisture\":100.0"));

    let minimum = SensorState {
        temperature: Some(-20.0),
        ..Default::default()
    };
    assert_eq!(render(&minimum).as_str(), "{\"temperature\":-20.0}");
}

#[test]
fn a_single_field_does_not_emit_a_leading_comma() {
    let only_humidity = SensorState {
        humidity: Some(40.0),
        ..Default::default()
    };
    assert_eq!(render(&only_humidity).as_str(), "{\"humidity\":40.0}");

    let only_soil = SensorState {
        soil_moisture: Some(48.0),
        ..Default::default()
    };
    assert_eq!(render(&only_soil).as_str(), "{\"soilMoisture\":48.0}");

    let empty = SensorState::default();
    assert_eq!(render(&empty).as_str(), "{}");
    assert!(empty.is_empty());
}

#[test]
fn update_helpers_set_and_clean_the_state() {
    let mut state = SensorState::default();
    state.update_bme280(22.0, 50.0, 1_013.0);
    state.update_lux(1_000.0);
    state.update_ph(6.5);
    state.update_tds(700.0);
    state.update_soil_moisture(45.0);
    assert!(!state.is_empty());
    assert_eq!(state.temperature, Some(22.0));
    assert_eq!(state.atmosphere, Some(1_013.0));

    state.clean();
    assert!(state.is_empty());
    assert_eq!(render(&state).as_str(), "{}");
}
