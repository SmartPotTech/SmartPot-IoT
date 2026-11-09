//! Frases de consola para los fallos de red y del broker.
//!
//! Son las mismas que imprime el firmware MicroPython (`describe_error` y
//! `connect_failure`), así que el mismo fallo se lee igual en las dos consolas.
//! El módulo es puro: la traducción desde los errores de embassy-net,
//! embedded-io y rust-mqtt la hace la tarea MQTT.

/// Clase de fallo de red, independiente de la pila que lo detecta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetFailure {
    TimedOut,
    ConnectionReset,
    ConnectionAborted,
    ConnectionRefused,
    HostUnreachable,
    NotConnected,
    /// El broker cerró la conexión (fin de flujo o socket cerrado).
    ClosedByBroker,
    OutOfMemory,
    /// Paquete MQTT inesperado o que no cumple la especificación.
    Protocol,
    /// Cualquier otro fallo de la pila.
    Other,
}

impl NetFailure {
    pub const fn text(self) -> &'static str {
        match self {
            Self::TimedOut => "timed out",
            Self::ConnectionReset => "connection reset",
            Self::ConnectionAborted => "connection aborted",
            Self::ConnectionRefused => "connection refused",
            Self::HostUnreachable => "host unreachable",
            Self::NotConnected => "not connected",
            Self::ClosedByBroker => "connection closed by the broker",
            Self::OutOfMemory => "out of memory",
            Self::Protocol => "protocol error",
            Self::Other => "network error",
        }
    }
}

/// El broker rechazó la clave del dispositivo (CONNACK 0x86 o 0x87).
pub const KEY_REJECTED: &str = "broker rejected the device key";

/// El broker rechazó la conexión por otro motivo.
pub const BROKER_REFUSED: &str = "broker refused the connection";

/// El PINGREQ anterior quedó sin respuesta: la conexión está medio abierta.
pub const NO_PING_RESPONSE: &str = "no PINGRESP from the broker";

/// `config.json` no trae la CA del broker.
pub const CA_MISSING: &str = "Cannot connect securely: config.json is missing the broker CA: copy the BROKER block from the connection guide";

/// La CA de `config.json` no se puede leer.
pub const CA_CORRUPTED: &str =
    "Cannot connect securely: the CA in config.json is corrupted: copy the BROKER block again";

/// Primer intento de Wi-Fi agotado.
pub const NO_WIFI: &str = "No WiFi: device continues measuring and keeps retrying";

/// El LCD2004 (dirección 0x27) dejó de responder o no está conectado.
pub const LCD_UNAVAILABLE: &str = "LCD not available at 0x27";

/// Prefijo de los comandos rechazados antes de ejecutarse.
pub const COMMAND_REJECTED: &str = "Command rejected: ";
