//! Protocolo de comunicación IoT según el contrato v1 de SmartPot.

pub mod ack;
pub mod command;
pub mod telemetry;
pub mod topics;

pub use ack::{format_readable_duration, AckMessage, AckStatus};
pub use command::{parse_command, Action, Actuators, Command};
pub use telemetry::SensorState;
pub use topics::{TopicResolver, TOPIC_PREFIX};
