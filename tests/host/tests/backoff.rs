//! Pruebas de la espera exponencial con dispersión de los reintentos de red.

use smartpot_iot_host::backoff::Backoff;

#[test]
fn delays_grow_exponentially_until_the_cap() {
    let mut backoff = Backoff::new(2_000, 60_000);
    // Con aleatorio 0 se obtiene el suelo del intervalo: la mitad del techo.
    let floors: Vec<u32> = (0..7).map(|_| backoff.next_delay_ms(0)).collect();
    assert_eq!(floors, [1_000, 2_000, 4_000, 8_000, 16_000, 30_000, 30_000]);
}

#[test]
fn jitter_stays_inside_the_upper_half_of_the_window() {
    let mut backoff = Backoff::new(2_000, 60_000);
    for attempt in 0..20u32 {
        let random = attempt.wrapping_mul(2_654_435_761);
        let delay = backoff.next_delay_ms(random);
        let ceiling = (2_000u32 << attempt.min(16)).min(60_000);
        assert!(delay >= ceiling / 2 && delay <= ceiling, "attempt {attempt}: {delay}");
    }
}

#[test]
fn reset_starts_again_from_the_base() {
    let mut backoff = Backoff::new(2_000, 60_000);
    for _ in 0..5 {
        backoff.next_delay_ms(0);
    }
    assert_eq!(backoff.attempt(), 5);
    backoff.reset();
    assert_eq!(backoff.next_delay_ms(1_000), 2_000);
}
