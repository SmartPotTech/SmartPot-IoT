//! Resolución y formateo de tópicos MQTT según el contrato v1 de SmartPot.
//!
//! Tópicos por cultivo:
//! - Telemetría:  `smartpot/v1/{cropId}/telemetry`
//! - Comandos:    `smartpot/v1/{cropId}/commands`
//! - Confirmación:`smartpot/v1/{cropId}/commands/ack`
//! - Estado LWT:  `smartpot/v1/{cropId}/status`

use heapless::String;

pub const TOPIC_PREFIX: &str = "smartpot/v1";

pub struct TopicResolver {
    prefix: &'static str,
    crop_id: String<32>,
}

impl TopicResolver {
    pub fn new(crop_id: &str) -> Self {
        let mut id = String::new();
        let _ = id.push_str(crop_id);
        Self {
            prefix: TOPIC_PREFIX,
            crop_id: id,
        }
    }

    pub fn telemetry<const N: usize>(&self) -> String<N> {
        let mut s = String::new();
        use core::fmt::Write;
        let _ = write!(s, "{}/{}/telemetry", self.prefix, self.crop_id.as_str());
        s
    }

    pub fn commands<const N: usize>(&self) -> String<N> {
        let mut s = String::new();
        use core::fmt::Write;
        let _ = write!(s, "{}/{}/commands", self.prefix, self.crop_id.as_str());
        s
    }

    pub fn ack<const N: usize>(&self) -> String<N> {
        let mut s = String::new();
        use core::fmt::Write;
        let _ = write!(s, "{}/{}/commands/ack", self.prefix, self.crop_id.as_str());
        s
    }

    pub fn status<const N: usize>(&self) -> String<N> {
        let mut s = String::new();
        use core::fmt::Write;
        let _ = write!(s, "{}/{}/status", self.prefix, self.crop_id.as_str());
        s
    }

    pub fn is_command_topic(&self, topic: &str) -> bool {
        let mut expected: String<64> = String::new();
        use core::fmt::Write;
        let _ = write!(expected, "{}/{}/commands", self.prefix, self.crop_id.as_str());
        topic == expected.as_str()
    }
}
