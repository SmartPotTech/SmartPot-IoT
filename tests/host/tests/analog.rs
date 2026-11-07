//! Pruebas de las conversiones analógicas: rieles, pendiente de Nernst y promedio.

use smartpot_iot_host::tds::{average, ph_from_adc, soil_moisture_from_adc, tds_from_adc};

/// Lectura cruda del ADC para una tensión dada (VREF 3,3 V, 12 bits).
fn raw(volts: f32) -> u16 {
    (volts / 3.3 * 4095.0).round() as u16
}

#[test]
fn a_disconnected_ph_probe_is_not_reported_as_ph_0_or_14() {
    assert_eq!(ph_from_adc(0, 25.0), None);
    assert_eq!(ph_from_adc(4095, 25.0), None);
}

#[test]
fn ph_uses_the_calibrated_neutral_point_and_slope() {
    let neutral = ph_from_adc(raw(2.5), 25.0).unwrap();
    assert!((neutral - 7.0).abs() < 0.02, "{neutral}");
    let acidic = ph_from_adc(raw(2.5 + 0.18 * 3.0), 25.0).unwrap();
    assert!((acidic - 4.0).abs() < 0.05, "{acidic}");
}

#[test]
fn temperature_scales_the_nernst_slope_not_the_voltage() {
    // En el punto neutro la temperatura no cambia el pH.
    let cold = ph_from_adc(raw(2.5), 5.0).unwrap();
    let hot = ph_from_adc(raw(2.5), 45.0).unwrap();
    assert!((cold - hot).abs() < 0.02);
    // Lejos del neutro, una solución más caliente tiene más pendiente: el
    // mismo voltaje corresponde a un pH más cercano a 7.
    let cold_acid = ph_from_adc(raw(3.0), 5.0).unwrap();
    let hot_acid = ph_from_adc(raw(3.0), 45.0).unwrap();
    assert!(hot_acid > cold_acid);
}

#[test]
fn tds_accepts_pure_water_but_not_a_saturated_probe() {
    assert_eq!(tds_from_adc(0, 25.0), Some(0.0));
    assert_eq!(tds_from_adc(4095, 25.0), None);
    let reading = tds_from_adc(raw(1.0), 25.0).unwrap();
    assert!(reading > 300.0 && reading < 400.0, "{reading}");
}

#[test]
fn soil_moisture_stays_in_percent() {
    assert_eq!(soil_moisture_from_adc(0), 0.0);
    assert_eq!(soil_moisture_from_adc(4095), 100.0);
}

#[test]
fn average_ignores_nothing_and_handles_empty_input() {
    assert_eq!(average(&[100, 200, 300]), Some(200));
    assert_eq!(average(&[]), None);
}
