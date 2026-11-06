//! Pruebas de comportamiento de los filtros: rechazo de espurias de arranque,
//! ventana de mediana sembrada, adaptación a escalones y saturación sin pánico.

use smartpot_iot_host::filter::{FloatKalmanFilter, MedianFilter5, Q16KalmanFilter, SensorCascade};

fn q16(value: f32) -> i32 {
    (value * Q16KalmanFilter::SCALE as f32) as i32
}

#[test]
fn median_filter_seeds_all_five_slots() {
    // Sembrar la ventana completa evita que los ceros iniciales contaminen la
    // mediana y que una espuria posterior la domine.
    let mut filter = MedianFilter5::seeded(25.0);
    // Con las cinco celdas sembradas hacen falta tres muestras bajas para
    // mover la mediana; una sola espuria no la desplaza.
    assert_eq!(filter.update(1.0), 25.0);
    assert_eq!(filter.update(2.0), 25.0);
    assert_eq!(filter.update(3.0), 3.0);

    // `new()` siembra las cinco celdas con la primera muestra.
    let mut fresh = MedianFilter5::new();
    assert_eq!(fresh.update(10.0), 10.0);
    assert_eq!(fresh.update(100.0), 10.0);
    assert_eq!(fresh.update(100.0), 10.0);
}

#[test]
fn startup_spike_is_rejected() {
    let mut median = MedianFilter5::seeded(25.0);
    assert_eq!(median.update(5_000.0), 25.0);
    assert_eq!(median.update(5_000.0), 25.0);

    let mut cascade = SensorCascade::new(0.01, 0.1);
    assert_eq!(cascade.update(25.0), 25.0);
    for _ in 0..3 {
        let estimate = cascade.update(5_000.0);
        assert!(
            (estimate - 25.0).abs() < 0.5,
            "startup spike moved the estimate to {estimate}"
        );
    }
}

#[test]
fn three_consecutive_spikes_are_rejected() {
    // La mediana sola cede tras dos espurias; la puerta de 3 sigma del Kalman
    // es la que mantiene la estimación en el valor real.
    let mut cascade = SensorCascade::new(0.01, 0.1);
    for _ in 0..20 {
        cascade.update(25.0);
    }

    for spike in 0..3 {
        let estimate = cascade.update(5_000.0);
        assert!(
            (estimate - 25.0).abs() < 0.5,
            "spike {spike} moved the estimate to {estimate}"
        );
    }

    let mut kalman = Q16KalmanFilter::from_f32(25.0, 1.0, 0.01, 0.1);
    for _ in 0..20 {
        kalman.update(q16(25.0));
    }
    for _ in 0..3 {
        let estimate = kalman.update(q16(5_000.0));
        assert!((estimate as f32 / Q16KalmanFilter::SCALE as f32 - 25.0).abs() < 0.5);
    }

    let mut float_filter = FloatKalmanFilter::new(25.0, 1.0, 0.01, 0.1);
    for _ in 0..20 {
        float_filter.update(25.0);
    }
    for _ in 0..3 {
        assert!((float_filter.update(5_000.0) - 25.0).abs() < 0.5);
    }
}

#[test]
fn innovation_gate_is_three_sigma() {
    // Con p = 1 y r = 1 la desviación es sqrt(2) y la puerta queda en 4.24.
    let mut kalman = Q16KalmanFilter::from_f32(0.0, 1.0, 0.0, 1.0);
    let accepted = kalman.update(q16(3.0));
    assert!(accepted != 0, "a 3.0 innovation must pass the 3 sigma gate");

    let before = kalman.value_q16();
    let rejected = kalman.update(q16(1_000.0));
    assert_eq!(
        rejected, before,
        "an innovation above 3 sigma must be rejected"
    );
}

#[test]
fn step_response_eventually_adapts() {
    let mut kalman = Q16KalmanFilter::from_f32(0.0, 1.0, 10.0, 0.1);
    for _ in 0..2_000 {
        kalman.update(q16(100.0));
    }
    assert!(
        (kalman.value_f32() - 100.0).abs() < 5.0,
        "q16 filter did not adapt: {}",
        kalman.value_f32()
    );

    let mut cascade = SensorCascade::new(10.0, 0.1);
    for _ in 0..20 {
        cascade.update(20.0);
    }
    for _ in 0..2_000 {
        cascade.update(120.0);
    }
    assert!(
        (cascade.value() - 120.0).abs() < 5.0,
        "cascade did not adapt: {}",
        cascade.value()
    );

    let mut float_filter = FloatKalmanFilter::new(0.0, 1.0, 10.0, 0.1);
    for _ in 0..2_000 {
        float_filter.update(100.0);
    }
    assert!((float_filter.value() - 100.0).abs() < 5.0);
}

#[test]
fn extreme_values_never_panic_and_stay_finite() {
    let mut kalman = Q16KalmanFilter::from_f32(f32::MAX, f32::MAX, f32::MAX, f32::MAX);
    for _ in 0..100 {
        kalman.update(i32::MAX);
        kalman.update(i32::MIN);
    }
    assert!(kalman.value_f32().is_finite());

    let mut cascade = SensorCascade::new(f32::MAX, f32::MAX);
    for value in [
        f32::MAX,
        f32::MIN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        0.0,
        -0.0,
    ] {
        let estimate = cascade.update(value);
        assert!(estimate.is_finite(), "non-finite estimate for {value}");
    }
    assert!(cascade.value().is_finite());

    let mut float_filter = FloatKalmanFilter::new(f32::MAX, f32::MAX, f32::MAX, f32::MAX);
    for value in [f32::MAX, f32::MIN, f32::NAN, f32::INFINITY, 0.0] {
        let _ = float_filter.update(value);
    }

    let mut median = MedianFilter5::new();
    for value in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MAX,
        f32::MIN,
    ] {
        let _ = median.update(value);
    }
}

#[test]
fn reset_clears_the_state() {
    let mut cascade = SensorCascade::new(0.01, 0.1);
    cascade.update(42.0);
    assert_ne!(cascade.value(), 0.0);
    cascade.reset();
    assert_eq!(cascade.value(), 0.0);

    let mut median = MedianFilter5::seeded(99.0);
    median.reset();
    assert_eq!(median.update(7.0), 7.0);
}
