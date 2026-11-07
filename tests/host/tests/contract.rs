//! Contrato MQTT v1 compartido con el simulador y el firmware MicroPython.
//!
//! La fixture fija los nombres canónicos de sensores y actuadores, los formatos
//! de telemetría y el alias heredado. Si el firmware cambia una capacidad sin
//! actualizar el contrato, estas pruebas fallan.

use serde_json::Value;
use smartpot_iot_host::capabilities::CAPABILITIES_JSON;
use smartpot_iot_host::protocol::Actuators;

const CONTRACT: &str = include_str!("fixtures/mqtt_v1_contract.json");

/// Actuadores cableados en esta placa; el firmware no debe declarar los dosificadores.
const INSTALLED_ACTUATORS: [&str; 4] = ["WATER_PUMP", "UV_LIGHT", "FAN", "HUMIDIFIER"];

fn contract() -> Value {
    serde_json::from_str(CONTRACT).expect("contract fixture is valid JSON")
}

fn capabilities() -> Value {
    serde_json::from_str(CAPABILITIES_JSON).expect("capabilities are valid JSON")
}

#[test]
fn capabilities_use_only_contract_sensor_names_and_units() {
    let contract = contract();
    let capabilities = capabilities();
    let expected_sensors = contract["sensors"].as_array().unwrap();
    let sensors = capabilities["sensors"].as_array().unwrap();

    assert_eq!(capabilities["schema"], contract["schema"]);
    assert_eq!(sensors.len(), expected_sensors.len());

    for sensor in sensors {
        let key = sensor["key"].as_str().unwrap();
        let expected = expected_sensors
            .iter()
            .find(|expected| expected["key"] == key)
            .unwrap_or_else(|| panic!("sensor {key} is not part of the MQTT v1 contract"));
        assert_eq!(
            sensor["unit"], expected["unit"],
            "unit changed for sensor {key}"
        );
    }
}

#[test]
fn capabilities_declare_only_canonical_actuator_names() {
    let contract = contract();
    let capabilities = capabilities();
    let contract_actuators: Vec<&str> = contract["actuators"]
        .as_array()
        .unwrap()
        .iter()
        .map(|actuator| actuator.as_str().unwrap())
        .collect();
    let declared: Vec<&str> = capabilities["actuators"]
        .as_array()
        .unwrap()
        .iter()
        .map(|actuator| actuator.as_str().unwrap())
        .collect();

    assert_eq!(declared, INSTALLED_ACTUATORS);
    for actuator in &declared {
        assert!(
            contract_actuators.contains(actuator),
            "actuator {actuator} is not part of the MQTT v1 contract"
        );
        assert_ne!(*actuator, "LIGHTS", "LIGHTS is an alias, not a capability");
    }
}

#[test]
fn telemetry_formats_and_compact_size_match_the_contract() {
    let contract = contract();
    let capabilities = capabilities();
    let telemetry = &capabilities["device"]["telemetry"];

    assert_eq!(telemetry["formats"], contract["telemetry"]["formats"]);
    assert_eq!(
        telemetry["compactBytes"],
        contract["telemetry"]["compactBytes"]
    );
    assert!(
        capabilities["device"]["firmware"]
            .as_str()
            .unwrap()
            .starts_with("smartpot-iot/")
    );
}

#[test]
fn every_contract_alias_resolves_to_its_canonical_actuator() {
    let contract = contract();
    let aliases = contract["aliases"].as_object().unwrap();

    for (alias, canonical) in aliases {
        let canonical_name = canonical.as_str().unwrap();
        let expected = Actuators::from_str(canonical_name)
            .unwrap_or_else(|_| panic!("canonical actuator {canonical_name} is not parseable"));
        assert_eq!(
            Actuators::from_str(alias).unwrap(),
            expected,
            "alias {alias} does not resolve to {canonical_name}"
        );
    }
}
