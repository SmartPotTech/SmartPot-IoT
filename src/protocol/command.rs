//! Procesamiento y deserialización de comandos MQTT según el contrato v1 de SmartPot.

use defmt::Format;
use heapless::String;

/// Identificador máximo aceptado para un comando (lo fija el contrato v1).
pub const MAX_COMMAND_ID_LEN: usize = 36;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Format)]
pub enum Actuators {
    WaterPump,
    UvLight,
    Fan,
    Humidifier,
    /// Dosificador de pH: el contrato lo admite, pero esta placa no lo tiene.
    PhDoser,
    /// Dosificador de nutrientes: el contrato lo admite, pero esta placa no lo tiene.
    NutrientDoser,
}

impl Actuators {
    #[allow(clippy::should_implement_trait, clippy::result_unit_err)]
    pub fn from_str(s: &str) -> Result<Self, ()> {
        let s = s.trim();
        if s.is_empty() {
            return Err(());
        }

        if s.eq_ignore_ascii_case("WATER_PUMP") {
            Ok(Self::WaterPump)
        } else if s.eq_ignore_ascii_case("UV_LIGHT") || s.eq_ignore_ascii_case("LIGHTS") {
            // `LIGHTS` es el alias heredado del contrato anterior.
            Ok(Self::UvLight)
        } else if s.eq_ignore_ascii_case("FAN") {
            Ok(Self::Fan)
        } else if s.eq_ignore_ascii_case("HUMIDIFIER") {
            Ok(Self::Humidifier)
        } else if s.eq_ignore_ascii_case("PH_DOSER") {
            Ok(Self::PhDoser)
        } else if s.eq_ignore_ascii_case("NUTRIENT_DOSER") {
            Ok(Self::NutrientDoser)
        } else {
            Err(())
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::WaterPump => "Water pump",
            Self::UvLight => "UV light",
            Self::Fan => "Fan",
            Self::Humidifier => "Humidifier",
            Self::PhDoser => "pH doser",
            Self::NutrientDoser => "Nutrient doser",
        }
    }

    /// Indica si el actuador es un dosificador no instalado en esta placa.
    pub fn is_doser(&self) -> bool {
        matches!(self, Self::PhDoser | Self::NutrientDoser)
    }

    /// Mensaje de rechazo para un dosificador que no está cableado a GPIO.
    pub fn unavailable_message(&self) -> Option<&'static str> {
        match self {
            Self::PhDoser => Some("This device does not have PH_DOSER installed"),
            Self::NutrientDoser => Some("This device does not have NUTRIENT_DOSER installed"),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Format)]
pub enum Action {
    Activate,
    Deactivate,
}

impl Action {
    #[allow(clippy::should_implement_trait, clippy::result_unit_err)]
    pub fn from_str(s: &str) -> Result<Self, ()> {
        let s = s.trim();
        if s.is_empty() {
            return Err(());
        }

        if s.eq_ignore_ascii_case("ACTIVATE") {
            Ok(Self::Activate)
        } else if s.eq_ignore_ascii_case("DEACTIVATE") {
            Ok(Self::Deactivate)
        } else {
            Err(())
        }
    }
}

#[derive(Clone, Debug, Format)]
pub struct Command {
    pub id: String<MAX_COMMAND_ID_LEN>,
    pub actuator: Actuators,
    pub action: Action,
    pub duration_seconds: Option<u32>,
}

/// Extrae el valor de un campo tipo cadena de un JSON simple sin asignaciones en memoria dinámica.
#[allow(clippy::result_unit_err)]
pub fn extract_str_field(json: &str, field: &str) -> Result<String<64>, ()> {
    let key = field.as_bytes();
    let bytes = json.as_bytes();
    let mut i = 0;

    while i + key.len() + 2 < bytes.len() {
        if bytes[i] == b'"'
            && &bytes[i + 1..i + 1 + key.len()] == key
            && bytes[i + 1 + key.len()] == b'"'
        {
            let mut j = i + 1 + key.len() + 1;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b':' || bytes[j] == b'\t') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'"' {
                j += 1;
                let start = j;
                while j < bytes.len() && bytes[j] != b'"' {
                    j += 1;
                }
                let val = core::str::from_utf8(&bytes[start..j]).map_err(|_| ())?;
                let mut res = String::new();
                res.push_str(val).map_err(|_| ())?;
                return Ok(res);
            }
        }
        i += 1;
    }
    Err(())
}

/// Extrae el valor numérico entero de un campo JSON sin asignaciones.
pub fn extract_u32_field(json: &str, field: &str) -> Option<u32> {
    let key = field.as_bytes();
    let bytes = json.as_bytes();
    let mut i = 0;

    while i + key.len() + 2 < bytes.len() {
        if bytes[i] == b'"'
            && &bytes[i + 1..i + 1 + key.len()] == key
            && bytes[i + 1 + key.len()] == b'"'
        {
            let mut j = i + 1 + key.len() + 1;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b':' || bytes[j] == b'\t') {
                j += 1;
            }
            // Podría ser número directo o entre comillas
            let in_quotes = if j < bytes.len() && bytes[j] == b'"' {
                j += 1;
                true
            } else {
                false
            };

            // Una duración negativa no es válida: no se recorta el signo y se
            // deja el campo como ausente en vez de convertir -5 en 5.
            if j < bytes.len() && bytes[j] == b'-' {
                return None;
            }

            let start = j;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if start < j
                && let Some(n) = core::str::from_utf8(&bytes[start..j])
                    .ok()
                    .and_then(|s| s.parse::<u32>().ok())
            {
                return Some(n);
            }
            let _ = in_quotes;
        }
        i += 1;
    }
    None
}

/// Parsea un payload JSON entrante en un comando estructurado.
///
/// Rechaza identificadores vacíos o de más de 36 caracteres, actuadores o
/// acciones vacíos y cualquier valor desconocido. Los dosificadores son
/// válidos a nivel de contrato: la tarea de comandos los rechaza con un ACK
/// descriptivo porque esta placa no los tiene instalados.
#[allow(clippy::result_unit_err)]
pub fn parse_command(payload: &str) -> Result<Command, ()> {
    let id_str = extract_str_field(payload, "id")?;
    let actuator_str = extract_str_field(payload, "actuator")?;
    let action_str = extract_str_field(payload, "action")?;
    let duration = extract_u32_field(payload, "durationSeconds");

    if id_str.trim().is_empty() || id_str.len() > MAX_COMMAND_ID_LEN {
        return Err(());
    }
    if actuator_str.trim().is_empty() || action_str.trim().is_empty() {
        return Err(());
    }

    let actuator = Actuators::from_str(actuator_str.as_str())?;
    let action = Action::from_str(action_str.as_str())?;

    let mut id = String::new();
    id.push_str(id_str.as_str()).map_err(|_| ())?;

    Ok(Command {
        id,
        actuator,
        action,
        duration_seconds: duration,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    fn parse(json: &str) -> Result<Command, ()> {
        parse_command(json)
    }

    #[test]
    fn parses_canonical_uv_light() {
        let cmd = parse(r#"{"id":"cmd-1","actuator":"UV_LIGHT","action":"ACTIVATE"}"#).unwrap();
        assert_eq!(cmd.actuator, Actuators::UvLight);
        assert_eq!(cmd.action, Action::Activate);
        assert_eq!(cmd.actuator.name(), "UV light");
        assert!(!cmd.actuator.is_doser());
    }

    #[test]
    fn accepts_legacy_lights_alias() {
        let cmd = parse(r#"{"id":"cmd-2","actuator":"LIGHTS","action":"DEACTIVATE"}"#).unwrap();
        assert_eq!(cmd.actuator, Actuators::UvLight);
        assert_eq!(cmd.action, Action::Deactivate);
    }

    #[test]
    fn accepts_lowercase_values() {
        let cmd = parse(r#"{"id":"cmd-3","actuator":"uv_light","action":"activate"}"#).unwrap();
        assert_eq!(cmd.actuator, Actuators::UvLight);
        assert_eq!(cmd.action, Action::Activate);
    }

    #[test]
    fn parses_dosers_and_reports_unavailable() {
        let ph = parse(r#"{"id":"cmd-4","actuator":"PH_DOSER","action":"ACTIVATE"}"#).unwrap();
        assert_eq!(ph.actuator, Actuators::PhDoser);
        assert!(ph.actuator.is_doser());
        assert_eq!(
            ph.actuator.unavailable_message(),
            Some("This device does not have PH_DOSER installed")
        );

        let nutrients =
            parse(r#"{"id":"cmd-5","actuator":"nutrient_doser","action":"ACTIVATE"}"#).unwrap();
        assert_eq!(nutrients.actuator, Actuators::NutrientDoser);
        assert!(nutrients.actuator.is_doser());
        assert_eq!(
            nutrients.actuator.unavailable_message(),
            Some("This device does not have NUTRIENT_DOSER installed")
        );
    }

    #[test]
    fn rejects_empty_and_unknown_fields() {
        assert!(parse("").is_err());
        assert!(parse(r#"{"id":"","actuator":"FAN","action":"ACTIVATE"}"#).is_err());
        assert!(parse(r#"{"id":"  ","actuator":"FAN","action":"ACTIVATE"}"#).is_err());
        assert!(parse(r#"{"id":"x","actuator":"","action":"ACTIVATE"}"#).is_err());
        assert!(parse(r#"{"id":"x","actuator":"FAN","action":""}"#).is_err());
        assert!(parse(r#"{"id":"x","actuator":"LASER","action":"ACTIVATE"}"#).is_err());
        assert!(parse(r#"{"id":"x","actuator":"FAN","action":"START"}"#).is_err());
    }

    #[test]
    fn rejects_ids_longer_than_thirty_six_characters() {
        let too_long = "0123456789012345678901234567890123456"; // 37 caracteres
        assert_eq!(too_long.len(), 37);
        let payload = format!(
            r#"{{"id":"{}","actuator":"FAN","action":"ACTIVATE"}}"#,
            too_long
        );
        assert!(parse(&payload).is_err());

        let max = "012345678901234567890123456789012345"; // 36 caracteres
        assert_eq!(max.len(), 36);
        let payload = format!(r#"{{"id":"{}","actuator":"FAN","action":"ACTIVATE"}}"#, max);
        assert!(parse(&payload).is_ok());
    }

    #[test]
    fn accepts_duration_seconds() {
        let cmd = parse(
            r#"{"id":"cmd-6","actuator":"WATER_PUMP","action":"ACTIVATE","durationSeconds":90}"#,
        )
        .unwrap();
        assert_eq!(cmd.duration_seconds, Some(90));
    }

    #[test]
    fn rejects_negative_duration_instead_of_truncating_the_sign() {
        let cmd = parse(
            r#"{"id":"cmd-7","actuator":"WATER_PUMP","action":"ACTIVATE","durationSeconds":-15}"#,
        )
        .unwrap();
        assert_eq!(cmd.duration_seconds, None);
    }
}
