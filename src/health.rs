//! Latidos de las tareas críticas y marca de agua de la pila del núcleo 1.
//!
//! Cada tarea vigilada incrementa su contador en cada vuelta de su bucle. El
//! supervisor solo alimenta el watchdog si todos los contadores que ya
//! arrancaron siguen avanzando; una tarea que nunca latió (por ejemplo, la de
//! MQTT mientras no hay Wi-Fi, o una deshabilitada por `features`) no se exige.
//! La lógica es pura para poder probarla en el host.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// Tareas vigiladas por el supervisor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Beat {
    Telemetry,
    Mqtt,
    Analog,
    Bme280,
    Light,
    Command,
    Display,
}

/// Número de tareas vigiladas.
pub const BEAT_COUNT: usize = 7;

/// Nombres de las tareas en el mismo orden que `Beat`.
pub const BEAT_NAMES: [&str; BEAT_COUNT] = [
    "telemetry",
    "mqtt",
    "analog",
    "bme280",
    "light",
    "command",
    "display",
];

static BEATS: [AtomicU32; BEAT_COUNT] = [const { AtomicU32::new(0) }; BEAT_COUNT];

/// Registra una vuelta del bucle de la tarea indicada.
pub fn beat(which: Beat) {
    BEATS[which as usize].fetch_add(1, Ordering::Relaxed);
}

/// Copia de los contadores actuales.
pub fn counts() -> [u32; BEAT_COUNT] {
    core::array::from_fn(|i| BEATS[i].load(Ordering::Relaxed))
}

/// Seguimiento de la última vez que avanzó cada contador.
pub struct Liveness {
    last: [u32; BEAT_COUNT],
    changed_at_ms: [u64; BEAT_COUNT],
}

impl Default for Liveness {
    fn default() -> Self {
        Self::new()
    }
}

impl Liveness {
    pub const fn new() -> Self {
        Self {
            last: [0; BEAT_COUNT],
            changed_at_ms: [0; BEAT_COUNT],
        }
    }

    /// Actualiza el estado y devuelve la primera tarea detenida, si la hay.
    ///
    /// Una tarea está detenida si ya latió alguna vez y su contador no cambió
    /// durante más de `timeout_ms`.
    pub fn check(
        &mut self,
        counts: [u32; BEAT_COUNT],
        now_ms: u64,
        timeout_ms: u64,
    ) -> Option<usize> {
        let mut stalled = None;
        for (index, &count) in counts.iter().enumerate() {
            if count == 0 || count != self.last[index] {
                self.last[index] = count;
                self.changed_at_ms[index] = now_ms;
                continue;
            }
            if stalled.is_none() && now_ms.saturating_sub(self.changed_at_ms[index]) > timeout_ms {
                stalled = Some(index);
            }
        }
        stalled
    }
}

/// Patrón con el que se pinta la pila del núcleo 1 antes de arrancarlo.
pub const STACK_PAINT: u32 = 0xA5A5_A5A5;

static CORE1_STACK_BOTTOM: AtomicUsize = AtomicUsize::new(0);
static CORE1_STACK_WORDS: AtomicUsize = AtomicUsize::new(0);

/// Pinta la pila del núcleo 1 y la registra para medir su marca de agua.
///
/// # Safety
/// `bottom` debe apuntar a `words` palabras válidas y alineadas que nadie esté
/// usando todavía (se llama antes de arrancar el núcleo 1).
pub unsafe fn paint_core1_stack(bottom: *mut u32, words: usize) {
    for index in 0..words {
        // SAFETY: el llamador garantiza que el rango es válido y exclusivo.
        unsafe { core::ptr::write_volatile(bottom.add(index), STACK_PAINT) };
    }
    CORE1_STACK_BOTTOM.store(bottom as usize, Ordering::Relaxed);
    CORE1_STACK_WORDS.store(words, Ordering::Relaxed);
}

/// Bytes de la pila del núcleo 1 que nunca se han usado (marca de agua).
///
/// `esp-hal` copia el cierre de arranque del núcleo 1 cerca del fondo de su
/// pila, de modo que debajo de él queda un pequeño tramo intacto que no forma
/// parte del hueco libre. Se toma el tramo intacto más largo que no empieza en
/// el fondo: el que queda entre el cierre y la zona más profunda que alcanzó
/// la pila.
pub fn core1_stack_unused_bytes() -> Option<usize> {
    let bottom = CORE1_STACK_BOTTOM.load(Ordering::Relaxed) as *const u32;
    let words = CORE1_STACK_WORDS.load(Ordering::Relaxed);
    if bottom.is_null() {
        return None;
    }
    // SAFETY: rango registrado por `paint_core1_stack`; solo se leen palabras
    // alineadas, aunque el núcleo 1 siga usando la parte alta.
    let read = |index: usize| unsafe { core::ptr::read_volatile(bottom.add(index)) };
    Some(unused_gap_words(words, read) * 4)
}

/// Longitud del tramo de palabras con el patrón que representa el hueco libre.
/// Es pura para poder probarla en el host.
pub fn unused_gap_words(words: usize, read: impl Fn(usize) -> u32) -> usize {
    let mut first_run = 0;
    let mut best_inner_run = 0;
    let mut index = 0;
    while index < words {
        if read(index) != STACK_PAINT {
            index += 1;
            continue;
        }
        let start = index;
        while index < words && read(index) == STACK_PAINT {
            index += 1;
        }
        let run = index - start;
        if start == 0 {
            first_run = run;
        } else {
            best_inner_run = best_inner_run.max(run);
        }
    }
    // Sin cierre copiado (o si la pila ya lo alcanzó) solo existe el tramo inicial.
    if best_inner_run > 0 {
        best_inner_run
    } else {
        first_run
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_the_beat_order() {
        assert_eq!(BEAT_NAMES[Beat::Mqtt as usize], "mqtt");
        assert_eq!(BEAT_NAMES[Beat::Display as usize], "display");
    }
}
