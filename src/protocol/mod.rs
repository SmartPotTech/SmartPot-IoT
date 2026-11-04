//! Protocolo de comunicación IoT según el contrato v1 de SmartPot.

pub mod ack;
pub mod command;
pub mod telemetry;
pub mod topics;

pub use ack::{AckMessage, AckStatus, format_readable_duration};
pub use command::{Action, Actuators, Command, parse_command};
pub use telemetry::SensorState;
pub use topics::{TOPIC_PREFIX, TopicResolver};
