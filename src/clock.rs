//! Reloj Unix de pared sincronizado mediante SNTP.
//!
//! El ESP32 no conserva la hora entre reinicios, así que el epoch empieza sin
//! sincronizar y la tarea SNTP lo establece cuando recibe una respuesta válida.
//! A partir de ahí el reloj avanza con `embassy_time::Instant`.
//!
//! La referencia se guarda en milisegundos de 64 bits: con un contador de 32
//! bits el reloj retrocedía unos 49,7 días si SNTP no renovaba la referencia
//! (lo que pasaba con un cristal que adelanta). Las reglas de corrección están
//! en [`crate::sntp::reconcile_epoch`].
//!
//! El estado se protege con un mutex de sección crítica porque el objetivo
//! xtensa no ofrece atómicos de 64 bits; las lecturas son muy cortas.

use core::cell::Cell;

use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::Instant;

use crate::sntp;

#[derive(Clone, Copy)]
struct ClockState {
    /// Segundos Unix correspondientes al instante de referencia.
    epoch_seconds: u64,
    /// Milisegundos de `Instant` en el momento de la sincronización.
    reference_ms: u64,
}

impl ClockState {
    fn at(&self, now_ms: u64) -> u64 {
        self.epoch_seconds + now_ms.saturating_sub(self.reference_ms) / 1_000
    }
}

static CLOCK: Mutex<CriticalSectionRawMutex, Cell<Option<ClockState>>> =
    Mutex::new(Cell::new(None));

/// Devuelve los segundos Unix actuales, o `None` mientras no haya sincronía.
pub fn now() -> Option<u64> {
    let now_ms = Instant::now().as_millis();
    CLOCK.lock(|cell| cell.get().map(|state| state.at(now_ms)))
}

/// Indica si el reloj ya recibió una hora válida por SNTP.
pub fn is_synced() -> bool {
    CLOCK.lock(|cell| cell.get().is_some())
}

/// Fija el epoch Unix actual a partir de una respuesta SNTP válida.
pub fn set_epoch(seconds: u64) {
    let now_ms = Instant::now().as_millis();

    CLOCK.lock(|cell| {
        let current = cell.get().map(|state| state.at(now_ms));
        if let Some(epoch_seconds) = sntp::reconcile_epoch(current, seconds) {
            cell.set(Some(ClockState {
                epoch_seconds,
                reference_ms: now_ms,
            }));
        }
    });
}
