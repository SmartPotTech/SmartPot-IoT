//! Generación de confirmaciones de comando (ACK) según el contrato v1 de SmartPot.

use core::fmt::Write;
use defmt::Format;
use heapless::String;
use super::command::{Action, Actuators};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Format)]
pub enum AckStatus {
    Executed,
    Failed,
}

#[derive(Clone, Debug, Format)]
pub struct AckMessage {
    pub id: String<36>,
    pub status: AckStatus,
    pub message: String<96>,
}

pub fn format_readable_duration<const N: usize>(seconds: u32, buf: &mut String<N>) {
    if seconds >= 3600 && seconds % 3600 == 0 {
        let _ = write!(buf, "{} h", seconds / 3600);
    } else if seconds >= 60 && seconds % 60 == 0 {
        let _ = write!(buf, "{} min", seconds / 60);
    } else {
        let _ = write!(buf, "{} s", seconds);
    }
}

impl AckMessage {
    pub fn success(id: &str, actuator: Actuators, action: Action, duration: Option<u32>) -> Self {
        let mut msg: String<96> = String::new();
        let name = actuator.name_es();
        let ending = if actuator.is_feminine() { "a" } else { "o" };

        match action {
            Action::Activate => {
                if let Some(secs) = duration {
                    let mut dur_buf: String<16> = String::new();
                    format_readable_duration(secs, &mut dur_buf);
                    let _ = write!(msg, "{} encendid{} por {}", name, ending, dur_buf.as_str());
                } else {
                    let _ = write!(msg, "{} encendid{}", name, ending);
                }
            }
            Action::Deactivate => {
                let _ = write!(msg, "{} apagad{}", name, ending);
            }
        }

        let mut id_str = String::new();
        let _ = id_str.push_str(id);

        Self {
            id: id_str,
            status: AckStatus::Executed,
            message: msg,
        }
    }

    pub fn failure(id: &str, err_msg: &str) -> Self {
        let mut msg: String<96> = String::new();
        let _ = msg.push_str(err_msg);
        let mut id_str = String::new();
        let _ = id_str.push_str(id);
        Self {
            id: id_str,
            status: AckStatus::Failed,
            message: msg,
        }
    }

    pub fn to_json<const N: usize>(&self) -> String<N> {
        let mut buf = String::new();
        let status_str = match self.status {
            AckStatus::Executed => "EXECUTED",
            AckStatus::Failed => "FAILED",
        };
        let _ = write!(
            buf,
            "{{\"id\":\"{}\",\"status\":\"{}\",\"message\":\"{}\"}}",
            self.id.as_str(),
            status_str,
            self.message.as_str()
        );
        buf
    }
}
