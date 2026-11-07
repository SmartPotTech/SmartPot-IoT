//! Crate auxiliar para ejecutar las pruebas de los módulos puros del firmware
//! en el host (x86_64), ya que el ESP32 no puede ejecutar `cargo test`.
//!
//! Solo se incluyen módulos sin dependencias de hardware: límites, protocolo,
//! códec, filtros, capacidades y configuración. Los módulos se enlazan con
//! `#[path]` para probar exactamente el mismo código que compila el firmware.

extern crate alloc;

#[path = "../../../src/board.rs"]
pub mod board;

#[path = "../../../src/limits.rs"]
pub mod limits;

// `mod.rs` resuelve sus submodules (`ack`, `command`, ...) de forma relativa
// al directorio del archivo incluido, así que el árbol queda idéntico al real.
#[path = "../../../src/protocol/mod.rs"]
pub mod protocol;

#[path = "../../../src/codec.rs"]
pub mod codec;

#[path = "../../../src/filter.rs"]
pub mod filter;

#[path = "../../../src/capabilities.rs"]
pub mod capabilities;

#[path = "../../../src/config.rs"]
pub mod config;

#[path = "../../../src/safety.rs"]
pub mod safety;

#[path = "../../../src/health.rs"]
pub mod health;

#[path = "../../../src/backoff.rs"]
pub mod backoff;

#[path = "../../../src/sntp.rs"]
pub mod sntp;

#[path = "../../../src/drivers/tds.rs"]
pub mod tds;

#[path = "../../../src/display_layout.rs"]
pub mod display_layout;

#[path = "../../../src/drivers/dht22_frame.rs"]
pub mod dht22_frame;
