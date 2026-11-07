//! Pruebas del parser de comandos: nombres canónicos, alias heredado,
//! dosificadores no instalados, límite de 36 caracteres, valores inválidos y
//! política de seguridad de duraciones.

use smartpot_iot_host::protocol::command::MAX_COMMAND_ID_LEN;
use smartpot_iot_host::protocol::{
    AckMessage, Action, Actuators, CommandError, format_readable_duration, parse_command,
};
use smartpot_iot_host::safety;

fn parse(payload: &str) -> Result<smartpot_iot_host::protocol::Command, CommandError> {
    parse_command(payload)
}

#[test]
fn uv_light_is_the_canonical_actuator() {
    let command = parse(r#"{"id":"cmd-1","actuator":"UV_LIGHT","action":"ACTIVATE"}"#).unwrap();
    assert_eq!(command.actuator, Actuators::UvLight);
    assert_eq!(command.action, Action::Activate);
    assert_eq!(command.actuator.name(), "UV light");
    assert!(!command.actuator.is_doser());
    assert_eq!(command.actuator.unavailable_message(), None);
    assert_eq!(command.id.as_str(), "cmd-1");
    assert_eq!(command.duration_seconds, None);
}

#[test]
fn lights_is_accepted_as_a_legacy_alias() {
    let command = parse(r#"{"id":"cmd-2","actuator":"LIGHTS","action":"DEACTIVATE"}"#).unwrap();
    assert_eq!(command.actuator, Actuators::UvLight);
    assert_eq!(command.action, Action::Deactivate);
}

#[test]
fn dosers_parse_but_report_they_are_not_installed() {
    let ph = parse(r#"{"id":"cmd-3","actuator":"PH_DOSER","action":"ACTIVATE"}"#).unwrap();
    assert_eq!(ph.actuator, Actuators::PhDoser);
    assert!(ph.actuator.is_doser());
    assert_eq!(
        ph.actuator.unavailable_message(),
        Some("This device does not have PH_DOSER installed")
    );

    let nutrients =
        parse(r#"{"id":"cmd-4","actuator":"NUTRIENT_DOSER","action":"ACTIVATE"}"#).unwrap();
    assert_eq!(nutrients.actuator, Actuators::NutrientDoser);
    assert!(nutrients.actuator.is_doser());
    assert_eq!(
        nutrients.actuator.unavailable_message(),
        Some("This device does not have NUTRIENT_DOSER installed")
    );
}

#[test]
fn identifiers_are_limited_to_thirty_six_characters() {
    let max = "012345678901234567890123456789012345";
    assert_eq!(max.len(), MAX_COMMAND_ID_LEN);
    let payload = format!(r#"{{"id":"{max}","actuator":"FAN","action":"ACTIVATE"}}"#);
    let command = parse(&payload).unwrap();
    assert_eq!(command.id.len(), MAX_COMMAND_ID_LEN);
    assert_eq!(command.id.as_str(), max);

    let too_long = "0123456789012345678901234567890123456";
    assert_eq!(too_long.len(), MAX_COMMAND_ID_LEN + 1);
    let payload = format!(r#"{{"id":"{too_long}","actuator":"FAN","action":"ACTIVATE"}}"#);
    assert!(parse(&payload).is_err());
}

#[test]
fn invalid_actuators_actions_and_fields_are_rejected() {
    assert!(parse("").is_err());
    assert!(parse("{}").is_err());
    assert!(parse(r#"{"id":"","actuator":"FAN","action":"ACTIVATE"}"#).is_err());
    assert!(parse(r#"{"id":"   ","actuator":"FAN","action":"ACTIVATE"}"#).is_err());
    assert!(parse(r#"{"id":"x","actuator":"","action":"ACTIVATE"}"#).is_err());
    assert!(parse(r#"{"id":"x","actuator":"FAN","action":""}"#).is_err());
    assert!(parse(r#"{"id":"x","actuator":"LASER","action":"ACTIVATE"}"#).is_err());
    assert!(parse(r#"{"id":"x","actuator":"FAN","action":"START"}"#).is_err());
    assert!(parse(r#"{"id":"x","actuator":"FAN","action":123}"#).is_err());
    assert!(parse(r#"{"actuator":"FAN","action":"ACTIVATE"}"#).is_err());
    assert!(parse(r#"{"id":"x","action":"ACTIVATE"}"#).is_err());
}

#[test]
fn values_are_trimmed_and_case_insensitive() {
    let command =
        parse(r#"{ "id" : " cmd-5 " , "actuator" : " water_pump " , "action" : " Activate " }"#)
            .unwrap();
    assert_eq!(command.actuator, Actuators::WaterPump);
    assert_eq!(command.action, Action::Activate);

    let lights = parse(r#"{"id":"x","actuator":"lights","action":"deactivate"}"#).unwrap();
    assert_eq!(lights.actuator, Actuators::UvLight);
}

#[test]
fn duration_is_optional_and_parsed_as_u32() {
    let command =
        parse(r#"{"id":"cmd-6","actuator":"WATER_PUMP","action":"ACTIVATE","durationSeconds":90}"#)
            .unwrap();
    assert_eq!(command.duration_seconds, Some(90));

    let absent = parse(r#"{"id":"cmd-8","actuator":"FAN","action":"DEACTIVATE"}"#).unwrap();
    assert_eq!(absent.duration_seconds, None);

    // `null` equivale a omitir el campo, como hace la API con `omitempty`.
    let null = parse(r#"{"id":"cmd-9","actuator":"FAN","action":"ACTIVATE","durationSeconds":null}"#)
        .unwrap();
    assert_eq!(null.duration_seconds, None);

    let bounds = parse(
        r#"{"id":"cmd-10","actuator":"FAN","action":"ACTIVATE","durationSeconds" : 3600 }"#,
    )
    .unwrap();
    assert_eq!(bounds.duration_seconds, Some(3600));
}

#[test]
fn invalid_durations_are_rejected_with_a_reason() {
    // Antes, un valor negativo se tomaba como «sin duración» y el actuador
    // quedaba encendido indefinidamente.
    for value in ["-15", "0", "3601", "120.5", "1e2", "\"120\"", "false", "[]"] {
        let payload = format!(
            r#"{{"id":"cmd-11","actuator":"WATER_PUMP","action":"ACTIVATE","durationSeconds":{value}}}"#
        );
        let error = parse(&payload).unwrap_err();
        assert_eq!(error, CommandError::InvalidDuration, "durationSeconds={value}");
        assert_eq!(
            error.message(),
            "durationSeconds must be an integer between 1 and 3600"
        );
    }
}

#[test]
fn errors_carry_the_ack_reason() {
    assert_eq!(
        parse(r#"{"id":"x","actuator":"LASER","action":"ACTIVATE"}"#).unwrap_err(),
        CommandError::UnknownActuator
    );
    assert_eq!(
        parse(r#"{"id":"x","actuator":"FAN","action":"START"}"#).unwrap_err(),
        CommandError::UnknownAction
    );
    assert_eq!(
        parse(r#"{"actuator":"FAN","action":"ACTIVATE"}"#).unwrap_err(),
        CommandError::Malformed
    );
}

#[test]
fn critical_actuators_never_run_without_a_limit() {
    assert_eq!(
        safety::effective_duration(Actuators::WaterPump, None),
        Some(safety::MAX_DURATION_SECONDS)
    );
    assert_eq!(
        safety::effective_duration(Actuators::Humidifier, None),
        Some(safety::MAX_DURATION_SECONDS)
    );
    assert_eq!(safety::effective_duration(Actuators::WaterPump, Some(30)), Some(30));
    assert_eq!(safety::effective_duration(Actuators::UvLight, None), None);
    assert_eq!(safety::effective_duration(Actuators::Fan, None), None);
}

#[test]
fn failsafe_waits_for_the_full_offline_window() {
    let cutoff_ms = safety::OFFLINE_CUTOFF_SECONDS * 1000;
    // Con broker no hay failsafe, por mucho que lleve encendida.
    assert!(!safety::failsafe_due(Actuators::WaterPump, None, cutoff_ms * 10));
    // Sin broker cuenta el plazo más reciente entre el corte y el encendido.
    assert!(!safety::failsafe_due(Actuators::WaterPump, Some(cutoff_ms), cutoff_ms - 1));
    assert!(!safety::failsafe_due(Actuators::WaterPump, Some(cutoff_ms - 1), cutoff_ms));
    assert!(safety::failsafe_due(Actuators::WaterPump, Some(cutoff_ms), cutoff_ms));
    assert!(safety::failsafe_due(Actuators::Humidifier, Some(cutoff_ms * 2), cutoff_ms));
    // La luz UV y el ventilador nunca se apagan por el failsafe.
    assert!(!safety::failsafe_due(Actuators::UvLight, Some(cutoff_ms * 2), cutoff_ms * 2));
    assert!(!safety::failsafe_due(Actuators::Fan, Some(cutoff_ms * 2), cutoff_ms * 2));
}

#[test]
fn ack_json_escapes_special_characters() {
    let ack = AckMessage::failure("a\"b\\c", "bad");
    let json: heapless::String<160> = ack.to_json();
    let parsed: serde_json::Value = serde_json::from_str(json.as_str()).unwrap();
    assert_eq!(parsed["id"], "a\"b\\c");
    assert_eq!(parsed["status"], "FAILED");
}

#[test]
fn actuators_expose_their_contract_keys() {
    assert_eq!(Actuators::WaterPump.key(), "WATER_PUMP");
    assert_eq!(Actuators::UvLight.key(), "UV_LIGHT");
    assert_eq!(Actuators::Humidifier.key(), "HUMIDIFIER");
    assert_eq!(Actuators::NutrientDoser.key(), "NUTRIENT_DOSER");
}

#[test]
fn readable_duration_formats_hours_minutes_and_seconds() {
    let mut buffer = heapless::String::<16>::new();
    format_readable_duration(7200, &mut buffer);
    assert_eq!(buffer.as_str(), "2 h");

    let mut buffer = heapless::String::<16>::new();
    format_readable_duration(180, &mut buffer);
    assert_eq!(buffer.as_str(), "3 min");

    let mut buffer = heapless::String::<16>::new();
    format_readable_duration(90, &mut buffer);
    assert_eq!(buffer.as_str(), "90 s");

    // Si no es un número exacto de horas ni de minutos, se muestran segundos.
    let mut buffer = heapless::String::<16>::new();
    format_readable_duration(5_401, &mut buffer);
    assert_eq!(buffer.as_str(), "5401 s");
}
