//! Procesamiento y deserialización de comandos MQTT según el contrato v1 de SmartPot.

use defmt::Format;
use heapless::String;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Format)]
pub enum Actuators {
    WaterPump,
    Lights,
    Fan,
    Humidifier,
}

impl Actuators {
    pub fn from_str(s: &str) -> Result<Self, ()> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("WATER_PUMP") || s.eq_ignore_ascii_case("water_pump") {
            Ok(Self::WaterPump)
        } else if s.eq_ignore_ascii_case("LIGHTS") || s.eq_ignore_ascii_case("lights") {
            Ok(Self::Lights)
        } else if s.eq_ignore_ascii_case("FAN") || s.eq_ignore_ascii_case("fan") {
            Ok(Self::Fan)
        } else if s.eq_ignore_ascii_case("HUMIDIFIER") || s.eq_ignore_ascii_case("humidifier") {
            Ok(Self::Humidifier)
        } else {
            Err(())
        }
    }

    pub fn name_es(&self) -> &'static str {
        match self {
            Self::WaterPump => "Bomba de agua",
            Self::Lights => "Luz de cultivo",
            Self::Fan => "Ventilador",
            Self::Humidifier => "Humidificador",
        }
    }

    pub fn is_feminine(&self) -> bool {
        match self {
            Self::WaterPump | Self::Lights => true,
            Self::Fan | Self::Humidifier => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Format)]
pub enum Action {
    Activate,
    Deactivate,
}

impl Action {
    pub fn from_str(s: &str) -> Result<Self, ()> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("ACTIVATE") || s.eq_ignore_ascii_case("activate") {
            Ok(Self::Activate)
        } else if s.eq_ignore_ascii_case("DEACTIVATE") || s.eq_ignore_ascii_case("deactivate") {
            Ok(Self::Deactivate)
        } else {
            Err(())
        }
    }
}

#[derive(Clone, Debug, Format)]
pub struct Command {
    pub id: String<36>,
    pub actuator: Actuators,
    pub action: Action,
    pub duration_seconds: Option<u32>,
}

/// Extrae el valor de un campo tipo cadena de un JSON simple sin asignaciones en memoria dinámica.
pub fn extract_str_field(json: &str, field: &str) -> Result<String<64>, ()> {
    let key = field.as_bytes();
    let bytes = json.as_bytes();
    let mut i = 0;

    while i + key.len() + 2 < bytes.len() {
        if bytes[i] == b'"' && &bytes[i + 1..i + 1 + key.len()] == key && bytes[i + 1 + key.len()] == b'"' {
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
        if bytes[i] == b'"' && &bytes[i + 1..i + 1 + key.len()] == key && bytes[i + 1 + key.len()] == b'"' {
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

            let start = j;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if start < j {
                if let Ok(s) = core::str::from_utf8(&bytes[start..j]) {
                    if let Ok(n) = s.parse::<u32>() {
                        return Some(n);
                    }
                }
            }
            let _ = in_quotes;
        }
        i += 1;
    }
    None
}

/// Parsea un payload JSON entrante en un comando estructurado.
pub fn parse_command(payload: &str) -> Result<Command, ()> {
    let id_str = extract_str_field(payload, "id")?;
    let actuator_str = extract_str_field(payload, "actuator")?;
    let action_str = extract_str_field(payload, "action")?;
    let duration = extract_u32_field(payload, "durationSeconds");

    let actuator = Actuators::from_str(actuator_str.as_str())?;
    let action = Action::from_str(action_str.as_str())?;

    let mut id = String::new();
    let _ = id.push_str(id_str.as_str());

    Ok(Command {
        id,
        actuator,
        action,
        duration_seconds: duration,
    })
}
