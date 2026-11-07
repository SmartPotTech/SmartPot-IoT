//! Pruebas del seguimiento de latidos que decide si se alimenta el watchdog.

use smartpot_iot_host::health::{Beat, Liveness, BEAT_COUNT};

const TIMEOUT_MS: u64 = 120_000;

fn counts(values: &[(Beat, u32)]) -> [u32; BEAT_COUNT] {
    let mut counts = [0; BEAT_COUNT];
    for &(beat, value) in values {
        counts[beat as usize] = value;
    }
    counts
}

#[test]
fn tasks_that_never_beat_are_not_required() {
    let mut liveness = Liveness::new();
    // La tarea MQTT no late mientras no hay Wi-Fi: no debe forzar un reinicio.
    for minute in 0..30 {
        let now = minute * 60_000;
        let snapshot = counts(&[(Beat::Command, minute as u32 + 1)]);
        assert_eq!(liveness.check(snapshot, now, TIMEOUT_MS), None);
    }
}

#[test]
fn a_task_that_stops_beating_is_reported_after_the_timeout() {
    let mut liveness = Liveness::new();
    let alive = counts(&[(Beat::Command, 5), (Beat::Analog, 3)]);
    assert_eq!(liveness.check(alive, 0, TIMEOUT_MS), None);

    // El ejecutor de comandos sigue latiendo; la lectura analógica se detiene.
    let stalled = counts(&[(Beat::Command, 50), (Beat::Analog, 3)]);
    assert_eq!(liveness.check(stalled, TIMEOUT_MS, TIMEOUT_MS), None);
    assert_eq!(
        liveness.check(stalled, TIMEOUT_MS + 1, TIMEOUT_MS),
        Some(Beat::Analog as usize)
    );
}

#[test]
fn a_new_beat_resets_the_deadline() {
    let mut liveness = Liveness::new();
    assert_eq!(
        liveness.check(counts(&[(Beat::Mqtt, 1)]), 0, TIMEOUT_MS),
        None
    );
    assert_eq!(
        liveness.check(counts(&[(Beat::Mqtt, 2)]), 100_000, TIMEOUT_MS),
        None
    );
    assert_eq!(
        liveness.check(counts(&[(Beat::Mqtt, 2)]), 200_000, TIMEOUT_MS),
        None
    );
    assert_eq!(
        liveness.check(counts(&[(Beat::Mqtt, 2)]), 220_001, TIMEOUT_MS),
        Some(Beat::Mqtt as usize)
    );
}

#[test]
fn counter_wraparound_still_counts_as_progress() {
    let mut liveness = Liveness::new();
    assert_eq!(
        liveness.check(counts(&[(Beat::Command, u32::MAX)]), 0, TIMEOUT_MS),
        None
    );
    assert_eq!(
        liveness.check(counts(&[(Beat::Command, 1)]), 200_000, TIMEOUT_MS),
        None
    );
}

#[test]
fn stack_gap_skips_the_entry_closure_copied_near_the_bottom() {
    use smartpot_iot_host::health::{STACK_PAINT as P, unused_gap_words};
    // 2 palabras intactas bajo el cierre, 3 del cierre, 5 libres y el resto usadas.
    let stack = [P, P, 1, 2, 3, P, P, P, P, P, 9, 9];
    assert_eq!(unused_gap_words(stack.len(), |i| stack[i]), 5);
    // Sin cierre copiado, el hueco empieza en el fondo.
    let plain = [P, P, P, 9, 9];
    assert_eq!(unused_gap_words(plain.len(), |i| plain[i]), 3);
    // Una pila que llegó hasta el cierre solo deja el tramo inferior.
    let full = [P, 1, 2, 9, 9, 9];
    assert_eq!(unused_gap_words(full.len(), |i| full[i]), 1);
}
