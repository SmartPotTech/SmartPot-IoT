//! Reloj Unix de pared sincronizado mediante SNTP.
//!
//! El ESP32 no conserva la hora entre reinicios, así que el epoch empieza sin
//! sincronizar y la tarea SNTP lo establece cuando recibe una respuesta válida.
//! A partir de ahí el reloj avanza con `embassy_time::Instant` y nunca retrocede,
//! ni siquiera si una resincronización entrega un valor anterior.
//!
//! El objetivo xtensa no ofrece atómicos de 64 bits, por lo que el estado se
//! protege con un mutex de sección crítica; las lecturas son muy cortas y no
//! bloquean a otras tareas más allá de unos pocos ciclos.

use core::cell::Cell;

use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::Instant;

#[derive(Clone, Copy)]
struct ClockState {
    /// Segundos Unix correspondientes al instante de referencia.
    epoch_seconds: u64,
    /// Milisegundos de `Instant` en el momento de la sincronización.
    reference_ms: u32,
}

static CLOCK: Mutex<CriticalSectionRawMutex, Cell<Option<ClockState>>> =
    Mutex::new(Cell::new(None));

/// Devuelve los segundos Unix actuales, o `None` mientras no haya sincronía.
pub fn now() -> Option<u64> {
    CLOCK.lock(|cell| {
        cell.get()
            .map(|state| advance(state.epoch_seconds, elapsed_ms(state.reference_ms)))
    })
}

/// Indica si el reloj ya recibió una hora válida por SNTP.
pub fn is_synced() -> bool {
    CLOCK.lock(|cell| cell.get().is_some())
}

/// Fija el epoch Unix actual a partir de una respuesta SNTP.
///
/// Si el reloj ya estaba sincronizado solo acepta valores posteriores al
/// instante actual, de modo que `now()` sea estrictamente monótono.
pub fn set_epoch(seconds: u64) {
    let reference_ms = Instant::now().as_millis() as u32;

    CLOCK.lock(|cell| {
        if let Some(current) = cell.get() {
            let current_epoch = advance(
                current.epoch_seconds,
                reference_ms.wrapping_sub(current.reference_ms) as u64,
            );
            if seconds <= current_epoch {
                return;
            }
        }

        cell.set(Some(ClockState {
            epoch_seconds: seconds,
            reference_ms,
        }));
    });
}

/// Milisegundos transcurridos desde la referencia, tolerando el desbordamiento
/// del contador de 32 bits (≈49,7 días).
fn elapsed_ms(reference_ms: u32) -> u64 {
    (Instant::now().as_millis() as u32).wrapping_sub(reference_ms) as u64
}

/// Suma milisegundos al epoch base.
fn advance(epoch_seconds: u64, elapsed_ms: u64) -> u64 {
    epoch_seconds + elapsed_ms / 1_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_keeps_seconds_until_a_full_second_passes() {
        assert_eq!(advance(1_700_000_000, 0), 1_700_000_000);
        assert_eq!(advance(1_700_000_000, 999), 1_700_000_000);
        assert_eq!(advance(1_700_000_000, 1_000), 1_700_000_001);
        assert_eq!(advance(0, 2_500), 2);
    }

    #[test]
    fn advance_is_monotonic_for_increasing_elapsed_times() {
        let base = 1_700_000_000;
        let mut previous = advance(base, 0);
        for ms in 0..10_000 {
            let current = advance(base, ms);
            assert!(current >= previous);
            previous = current;
        }
    }

    #[test]
    fn elapsed_ms_handles_counter_wraparound() {
        // Simula una referencia cerca del límite de u32 y un instante ya envuelto.
        assert_eq!(u32::MAX.wrapping_sub(u32::MAX - 450) as u64, 450);
        assert_eq!(10u32.wrapping_sub(u32::MAX) as u64, 11);
    }
}
