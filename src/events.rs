//! Canales y tipos de eventos para la comunicación entre tareas del firmware.

use defmt::Format;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::pubsub::PubSubChannel;

pub use crate::protocol::{
    format_readable_duration, parse_command, AckMessage, AckStatus, Action, Actuators, Command,
    SensorState, TopicResolver, TOPIC_PREFIX,
};

#[derive(Clone, Copy, Debug, Format)]
pub enum Measurements {
    BME280((f32, f32, f32)),
    BH1750(f32),
    PH(f32),
    TDS(f32),
    SoilMoisture(f32),
}
#[cfg(feature = "sensors")]
unsafe impl Send for Measurements {}

#[cfg(feature = "sensors")]
impl Measurements {
    pub fn get_topic(&self) -> &str {
        match self {
            Measurements::BME280(_) => "atmosphere",
            Measurements::BH1750(_) => "brightness",
            Measurements::PH(_) => "ph",
            Measurements::TDS(_) => "tds",
            Measurements::SoilMoisture(_) => "soilMoisture",
        }
    }
}

#[cfg(feature = "sensors")]
pub const SENSOR_CH_CAP: usize = 32;
#[cfg(feature = "sensors")]
pub const SENSOR_CH_PUB: usize = 4;
#[cfg(feature = "sensors")]
pub const SENSOR_CH_SUB: usize = 2;
#[cfg(feature = "sensors")]
pub static SENSOR_CH: PubSubChannel<
    CriticalSectionRawMutex,
    Measurements,
    SENSOR_CH_CAP,
    SENSOR_CH_SUB,
    SENSOR_CH_PUB,
> = PubSubChannel::new();

pub const COMMAND_CH_CAP: usize = 8;
pub const COMMAND_CH_PUB: usize = 2;
pub const COMMAND_CH_SUB: usize = 1;
pub static COMMAND_CH: PubSubChannel<
    CriticalSectionRawMutex,
    Command,
    COMMAND_CH_CAP,
    COMMAND_CH_SUB,
    COMMAND_CH_PUB,
> = PubSubChannel::new();

pub const ACK_CH_CAP: usize = 8;
pub const ACK_CH_PUB: usize = 2;
pub const ACK_CH_SUB: usize = 1;
pub static ACK_CH: PubSubChannel<
    CriticalSectionRawMutex,
    AckMessage,
    ACK_CH_CAP,
    ACK_CH_SUB,
    ACK_CH_PUB,
> = PubSubChannel::new();

pub mod telemetry {
    use super::Measurements;
    use crate::protocol::SensorState;
    use defmt::Format;
    use embassy_sync::{
        blocking_mutex::raw::CriticalSectionRawMutex, pubsub::PubSubChannel,
    };

    #[derive(Copy, Clone, Format)]
    pub enum TelemetryMessage {
        SendOne(Measurements),
        SendAll(SensorState),
    }

    pub const TELEMETRY_CH_CAP: usize = 8;
    pub const TELEMETRY_CH_SUB: usize = 2;
    pub const TELEMETRY_CH_PUB: usize = 2;

    pub static TELEMETRY_CH: PubSubChannel<
        CriticalSectionRawMutex,
        TelemetryMessage,
        TELEMETRY_CH_CAP,
        TELEMETRY_CH_SUB,
        TELEMETRY_CH_PUB,
    > = PubSubChannel::new();
}
