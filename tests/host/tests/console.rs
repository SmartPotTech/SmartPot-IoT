//! Salida de consola y LCD compartida con el firmware MicroPython.
//!
//! `fixtures/console_v1.json` es idéntico a `tests/fixtures/console_v1.json`
//! de SmartPot-IoT-Python y sus pruebas lo leen igual: si un firmware cambia un
//! texto, la tabla, una fila del LCD o un motivo de rechazo sin cambiar la
//! fixture, sus pruebas fallan y los dos dejan de verse igual.

use serde_json::Value;
use smartpot_iot_host::display_layout::{OFFLINE_GLYPH, ONLINE_GLYPH, render};
use smartpot_iot_host::protocol::{Action, CommandError, SensorState, parse_command};
use smartpot_iot_host::reason::{self, NetFailure};
use smartpot_iot_host::report::{DHT_CHECKSUM, DHT_NO_RESPONSE, dht_reason, reading_table};
use smartpot_iot_host::tds::linear_scale;

const FIXTURE: &str = include_str!("fixtures/console_v1.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("console fixture is valid JSON")
}

fn number(value: &Value) -> Option<f32> {
    value.as_f64().map(|v| v as f32)
}

fn state(readings: &Value) -> SensorState {
    SensorState {
        temperature: number(&readings["temperature"]),
        humidity: number(&readings["humidity"]),
        brightness: number(&readings["brightness"]),
        ph: number(&readings["ph"]),
        tds: number(&readings["tds"]),
        atmosphere: None,
        soil_moisture: number(&readings["soilMoisture"]),
    }
}

#[test]
fn analog_scale_matches_micropython() {
    for case in fixture()["analogScale"].as_array().unwrap() {
        let raw = case["raw"].as_u64().unwrap() as u16;
        let low = case["low"].as_f64().unwrap() as f32;
        let high = case["high"].as_f64().unwrap() as f32;
        let value = linear_scale(raw, low, high);
        assert_eq!(
            format!("{value:.2}"),
            case["table"].as_str().unwrap(),
            "raw {raw}"
        );
        let decimals = if high == 14.0 { 1 } else { 0 };
        assert_eq!(
            format!("{value:.decimals$}"),
            case["lcd"].as_str().unwrap(),
            "raw {raw}"
        );
    }
}

#[test]
fn reading_table_matches_micropython() {
    let table = &fixture()["table"];
    let lines: Vec<String> = reading_table(&state(&table["readings"]), table["epoch"].as_u64())
        .iter()
        .map(|line| line.as_str().to_owned())
        .collect();
    let expected: Vec<&str> = table["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line.as_str().unwrap())
        .collect();
    assert_eq!(lines, expected);
    let unsynchronized = reading_table(&SensorState::default(), None);
    assert_eq!(
        unsynchronized[1].as_str(),
        table["unsynchronizedHeader"].as_str().unwrap()
    );
}

#[test]
fn lcd_rows_match_micropython() {
    let case = &fixture()["lcd"];
    let actuators = &case["actuators"];
    let states = [
        actuators["WATER_PUMP"].as_bool().unwrap(),
        actuators["UV_LIGHT"].as_bool().unwrap(),
        actuators["FAN"].as_bool().unwrap(),
    ];
    let rows = render(
        &state(&case["readings"]),
        &states,
        case["online"].as_bool().unwrap(),
    );
    let icon = if case["icon"] == "online" {
        ONLINE_GLYPH
    } else {
        OFFLINE_GLYPH
    };
    let mut expected: Vec<String> = case["rowsWithoutIcon"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row.as_str().unwrap().to_owned())
        .collect();
    expected[0].push(icon);
    let rows: Vec<String> = rows.iter().map(|row| row.as_str().to_owned()).collect();
    assert_eq!(rows, expected);
}

#[test]
fn command_rejections_match_micropython() {
    for case in fixture()["commandRejections"].as_array().unwrap() {
        let payload = case["payload"].as_str().unwrap();
        let error = parse_command(payload).expect_err(payload);
        assert_eq!(
            error.reason(payload).as_str(),
            case["reason"].as_str().unwrap(),
            "{payload}"
        );
        let ack_id = CommandError::ack_id(payload);
        assert_eq!(
            ack_id.as_ref().map(|id| id.as_str()),
            case["ackId"].as_str(),
            "{payload}"
        );
    }
}

#[test]
fn accepted_commands_match_micropython() {
    for case in fixture()["acceptedCommands"].as_array().unwrap() {
        let payload = case["payload"].as_str().unwrap();
        let command = parse_command(payload).expect(payload);
        assert_eq!(command.actuator.key(), case["actuator"].as_str().unwrap());
        let action = match command.action {
            Action::Activate => "ACTIVATE",
            Action::Deactivate => "DEACTIVATE",
        };
        assert_eq!(action, case["action"].as_str().unwrap());
        assert_eq!(
            command.duration_seconds.map(u64::from),
            case["durationSeconds"].as_u64()
        );
    }
}

#[test]
fn console_messages_match_micropython() {
    let fixture = fixture();
    let messages = &fixture["messages"];
    let text = |key: &str| messages[key].as_str().unwrap().to_owned();
    assert_eq!(reason::NO_WIFI, text("noWifi"));
    assert_eq!(reason::LCD_UNAVAILABLE, text("lcdUnavailable"));
    assert_eq!(reason::CA_MISSING, text("caMissing"));
    assert_eq!(reason::CA_CORRUPTED, text("caCorrupted"));
    assert_eq!(
        format!("No connection to broker: {}", reason::KEY_REJECTED),
        text("keyRejected")
    );
    assert_eq!(
        format!("No connection to broker: {}", reason::BROKER_REFUSED),
        text("brokerRefused")
    );
    assert_eq!(
        format!("Error reading DHT22: {}", dht_reason(DHT_NO_RESPONSE)),
        text("dhtNoResponse")
    );
    assert_eq!(
        format!("Error reading DHT22: {}", dht_reason(DHT_CHECKSUM)),
        text("dhtChecksum")
    );
    assert_eq!(reason::COMMAND_REJECTED, text("commandRejected"));
    assert_eq!(
        format!("Connection lost: {}", reason::NO_PING_RESPONSE),
        text("noPingResponse")
    );
}

#[test]
fn network_failures_use_the_shared_phrases() {
    let fixture = fixture();
    let phrases: Vec<&str> = fixture["networkReasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|phrase| phrase.as_str().unwrap())
        .collect();
    for failure in [
        NetFailure::TimedOut,
        NetFailure::ConnectionReset,
        NetFailure::ConnectionAborted,
        NetFailure::ConnectionRefused,
        NetFailure::HostUnreachable,
        NetFailure::NotConnected,
        NetFailure::ClosedByBroker,
        NetFailure::OutOfMemory,
        NetFailure::Protocol,
    ] {
        assert!(phrases.contains(&failure.text()), "{:?}", failure);
    }
}
