//! Conversión de las lecturas analógicas (TDS, pH, humedad de suelo).
//!
//! Una sonda desconectada o en cortocircuito deja el ADC en uno de sus rieles.
//! Antes ese valor se recortaba al rango válido y se publicaba como pH 0 o 14
//! o TDS 3000; ahora la lectura se descarta (`None`). La calibración se puede
//! ajustar al compilar sin tocar el código.

use crate::config::parse_u64_or;

const VREF: f32 = 3.3;
const ADC_MAX: u16 = 4095;

/// Lecturas a menos de esta distancia de un riel se consideran sonda desconectada.
const RAIL_MARGIN: u16 = 16;

/// Tensión de la sonda de pH en pH 7, en milivoltios (`PH_NEUTRAL_MV`).
const PH_NEUTRAL_MV: u64 = match option_env!("PH_NEUTRAL_MV") {
    Some(v) => parse_u64_or(v, 2500),
    None => 2500,
};

/// Compensación de desplazamiento en milivoltios para calibración de pH (`PH_OFFSET_MV`).
const PH_OFFSET_MV: i64 = match option_env!("PH_OFFSET_MV") {
    Some(v) => crate::config::parse_i64_or(v, 0),
    None => 0,
};

/// Pendiente del módulo de pH a 25 °C, en milivoltios por unidad (`PH_SLOPE_MV`).
const PH_SLOPE_MV: u64 = match option_env!("PH_SLOPE_MV") {
    Some(v) => parse_u64_or(v, 180),
    None => 180,
};

/// Lecturas crudas del sensor de suelo en seco y empapado (`ADC_DRY`/`SOIL_DRY_RAW`,
/// `ADC_WET`/`SOIL_WET_RAW`). Un sensor capacitivo suele dar más tensión en seco: basta
/// con invertir los valores.
const SOIL_DRY_RAW: u64 = match option_env!("ADC_DRY") {
    Some(v) => parse_u64_or(v, 0),
    None => match option_env!("SOIL_DRY_RAW") {
        Some(v) => parse_u64_or(v, 0),
        None => 0,
    },
};
const SOIL_WET_RAW: u64 = match option_env!("ADC_WET") {
    Some(v) => parse_u64_or(v, ADC_MAX as u64),
    None => match option_env!("SOIL_WET_RAW") {
        Some(v) => parse_u64_or(v, ADC_MAX as u64),
        None => ADC_MAX as u64,
    },
};

fn at_low_rail(adc_raw: u16) -> bool {
    adc_raw <= RAIL_MARGIN
}

fn at_high_rail(adc_raw: u16) -> bool {
    adc_raw >= ADC_MAX - RAIL_MARGIN
}

fn volts(adc_raw: u16) -> f32 {
    adc_raw as f32 * VREF / ADC_MAX as f32
}

/// Calcula el TDS (ppm) a partir de la lectura cruda del ADC de 12 bits.
///
/// El riel bajo es válido (agua destilada da 0 V); el alto indica una sonda
/// en cortocircuito o fuera de rango.
pub fn tds_from_adc(adc_raw: u16, temperature_c: f32) -> Option<f32> {
    if at_high_rail(adc_raw) {
        return None;
    }

    // Compensación de temperatura estándar del TDS: 2 % por °C respecto a 25 °C.
    let compensation = (1.0 + 0.02 * (temperature_c - 25.0)).max(0.1);
    let voltage = volts(adc_raw) / compensation;

    // Curva cúbica del sensor TDS analógico estándar.
    let tds = (133.42 * voltage * voltage * voltage - 255.86 * voltage * voltage
        + 857.39 * voltage)
        * 0.5;
    Some(tds.clamp(0.0, crate::limits::TDS.max))
}

/// Calcula el pH a partir de la lectura cruda del ADC de 12 bits.
///
/// La temperatura corrige la pendiente de Nernst (proporcional a la
/// temperatura absoluta), no la tensión: aplicar la compensación del TDS al pH
/// era físicamente incorrecto. Ambos rieles indican sonda desconectada.
pub fn ph_from_adc(adc_raw: u16, temperature_c: f32) -> Option<f32> {
    if at_low_rail(adc_raw) || at_high_rail(adc_raw) {
        return None;
    }

    let slope = (PH_SLOPE_MV as f32 / 1000.0) * (temperature_c + 273.15) / 298.15;
    let neutral = (PH_NEUTRAL_MV as f32 + PH_OFFSET_MV as f32) / 1000.0;
    let ph = 7.0 + (neutral - volts(adc_raw)) / slope.max(0.01);
    if !(0.0..=14.0).contains(&ph) {
        return None;
    }
    Some(ph)
}

/// Alias retrocompatible de `ph_from_adc`.
pub fn raw_to_ph(adc_raw: u16, temperature_c: f32) -> Option<f32> {
    ph_from_adc(adc_raw, temperature_c)
}

/// Calcula el porcentaje de humedad del sustrato (0.0..=100.0 %).
pub fn soil_moisture_from_adc(adc_raw: u16) -> f32 {
    let dry = SOIL_DRY_RAW as f32;
    let wet = SOIL_WET_RAW as f32;
    if (wet - dry).abs() < 1.0 {
        return 0.0;
    }
    ((adc_raw as f32 - dry) / (wet - dry) * 100.0).clamp(0.0, 100.0)
}

/// Promedia un conjunto de lecturas crudas del ADC.
pub fn average(samples: &[u16]) -> Option<u16> {
    if samples.is_empty() {
        return None;
    }
    let sum: u32 = samples.iter().map(|&s| s as u32).sum();
    Some((sum / samples.len() as u32) as u16)
}

/// Escala lineal de un potenciómetro, igual que `ADCSensor` del firmware
/// MicroPython: `low` con el cursor al mínimo y `high` al máximo.
pub fn linear_scale(adc_raw: u16, low: f32, high: f32) -> f32 {
    low + adc_raw.min(ADC_MAX) as f32 * (high - low) / ADC_MAX as f32
}
