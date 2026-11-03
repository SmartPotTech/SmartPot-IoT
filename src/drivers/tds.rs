//! Drivers y utilidades de conversión para sensores analógicos (TDS, pH, humedad de suelo).

/// Calcula el valor de TDS (Total Dissolved Solids en ppm) a partir de la lectura ADC del ESP32.
///
/// * `adc_raw`: valor crudo del ADC de 12 bits (0..=4095).
/// * `temperature_c`: temperatura del agua en grados Celsius (para compensación térmica).
#[inline]
pub fn tds_from_adc(adc_raw: u16, temperature_c: f32) -> f32 {
    const VREF: f32 = 3.3;

    // Conversión a voltaje en escala de 12 bits
    let voltage = (adc_raw as f32) * VREF / 4095.0;

    // Coeficiente de compensación de temperatura estándar (2 % por °C respecto a 25 °C)
    let compensation_coefficient = 1.0 + 0.02 * (temperature_c - 25.0);
    let comp_voltage = voltage / compensation_coefficient.max(0.1);

    // Curva cúbica para sensor TDS analógico estándar
    let tds = (133.42 * comp_voltage * comp_voltage * comp_voltage
        - 255.86 * comp_voltage * comp_voltage
        + 857.39 * comp_voltage)
        * 0.5;

    tds.clamp(0.0, 3000.0)
}

/// Calcula el valor de pH a partir de la lectura ADC del ESP32.
///
/// * `adc_raw`: valor crudo del ADC de 12 bits (0..=4095).
/// * `temperature_c`: temperatura de la solución en grados Celsius.
#[inline]
pub fn ph_from_adc(adc_raw: u16, temperature_c: f32) -> f32 {
    const VREF: f32 = 3.3;

    let voltage = (adc_raw as f32) * VREF / 4095.0;
    let compensation_coefficient = 1.0 + 0.02 * (temperature_c - 25.0);
    let comp_voltage = voltage / compensation_coefficient.max(0.1);

    // Ecuación lineal típica para sonda de pH con punto neutro en 2.5 V
    let ph = 7.0 + ((2.5 - comp_voltage) / 0.18);
    ph.clamp(0.0, 14.0)
}

/// Calcula el porcentaje de humedad del sustrato (0.0..=100.0 %) a partir del ADC.
///
/// * `adc_raw`: valor crudo del ADC de 12 bits (0..=4095).
#[inline]
pub fn soil_moisture_from_adc(adc_raw: u16) -> f32 {
    // Sensor resistivo/capacitivo: 0 = seco (máximo voltaje) a 4095 = sumergido (o viceversa según montaje)
    let percent = (adc_raw as f32 / 4095.0) * 100.0;
    percent.clamp(0.0, 100.0)
}
