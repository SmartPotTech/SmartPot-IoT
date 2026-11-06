//! Paquete compacto de telemetría v2 (27 bytes, big-endian): la alternativa binaria al JSON del contrato v1, en el
//! mismo tópico. La API acepta los dos; el firmware lo usa si se compila con `COMPACT_TELEMETRY`.
//!
//! - `[0]` 'P' (0x50) · `[1]` versión 2 · `[2..4]` secuencia u16 · `[4..8]` época u32 en s (0 = sin hora)
//! - `[8..10]` máscara de presencia u16: bit 0 temperatura, 1 humedad, 2 luz, 3 pH, 4 TDS, 5 presión, 6 sustrato
//! - `[10..12]` temperatura i16 ×100 °C · `[12..14]` humedad u16 ×100 % · `[14..18]` luz u32 lux
//! - `[18..20]` pH u16 ×100 · `[20..22]` TDS u16 ppm · `[22..24]` presión u16 ×10 hPa · `[24..26]` sustrato ×100 %
//! - `[26]` CRC-8 (polinomio 0x07) de los bytes 0..26
//!
//! Los valores se redondean de forma simétrica con `libm::roundf` (mitad fuera
//! de cero) y solo se marcan presentes si el entero escalado cae dentro del
//! rango de la API. Un valor ausente o fuera de rango deja su bit en 0 y sus
//! bytes en 0; no se satura.

use crate::limits::{self, ApiRange};
use crate::protocol::SensorState;

pub const COMPACT_BINARY_MAGIC: u8 = 0x50;
pub const COMPACT_BINARY_VERSION: u8 = 0x02;
pub const PACKET_SIZE: usize = 27;

/// Calcula el checksum CRC-8 (polinomio 0x07) sobre el slice dado.
pub fn calc_crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0x00;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            if (crc & 0x80) != 0 {
                crc = (crc << 1) ^ 0x07;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

/// Codifica el estado de los sensores en `out` y devuelve la máscara de presencia (0 = nada que enviar).
pub fn encode_state(
    state: &SensorState,
    seq_id: u16,
    timestamp_epoch: u32,
    out: &mut [u8; PACKET_SIZE],
) -> u16 {
    *out = [0u8; PACKET_SIZE];
    out[0] = COMPACT_BINARY_MAGIC;
    out[1] = COMPACT_BINARY_VERSION;
    out[2..4].copy_from_slice(&seq_id.to_be_bytes());
    out[4..8].copy_from_slice(&timestamp_epoch.to_be_bytes());

    let mut mask: u16 = 0;

    if let Some(scaled) = scaled(state.temperature, limits::TEMPERATURE, 100.0) {
        mask |= 1 << 0;
        out[10..12].copy_from_slice(&(scaled as i16).to_be_bytes());
    }
    if let Some(scaled) = scaled(state.humidity, limits::HUMIDITY, 100.0) {
        mask |= 1 << 1;
        out[12..14].copy_from_slice(&(scaled as u16).to_be_bytes());
    }
    if let Some(scaled) = scaled(state.brightness, limits::BRIGHTNESS, 1.0) {
        mask |= 1 << 2;
        out[14..18].copy_from_slice(&(scaled as u32).to_be_bytes());
    }
    if let Some(scaled) = scaled(state.ph, limits::PH, 100.0) {
        mask |= 1 << 3;
        out[18..20].copy_from_slice(&(scaled as u16).to_be_bytes());
    }
    if let Some(scaled) = scaled(state.tds, limits::TDS, 1.0) {
        mask |= 1 << 4;
        out[20..22].copy_from_slice(&(scaled as u16).to_be_bytes());
    }
    if let Some(scaled) = scaled(state.atmosphere, limits::ATMOSPHERE, 10.0) {
        mask |= 1 << 5;
        out[22..24].copy_from_slice(&(scaled as u16).to_be_bytes());
    }
    if let Some(scaled) = scaled(state.soil_moisture, limits::SOIL_MOISTURE, 100.0) {
        mask |= 1 << 6;
        out[24..26].copy_from_slice(&(scaled as u16).to_be_bytes());
    }

    out[8..10].copy_from_slice(&mask.to_be_bytes());
    out[PACKET_SIZE - 1] = calc_crc8(&out[..PACKET_SIZE - 1]);
    mask
}

/// Escala, redondea y valida un valor contra el rango de la API.
///
/// Devuelve `None` si el valor no es finito o si el entero redondeado queda
/// fuera de los límites aceptados; en ese caso el campo se omite del paquete.
fn scaled(value: Option<f32>, range: ApiRange, scale: f32) -> Option<f32> {
    let value = value.filter(|v| v.is_finite())?;
    let rounded = libm::roundf(value * scale);

    if range.contains_scaled(rounded, scale) {
        Some(rounded)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(state: &SensorState, seq: u16, epoch: u32) -> ([u8; PACKET_SIZE], u16) {
        let mut packet = [0u8; PACKET_SIZE];
        let mask = encode_state(state, seq, epoch, &mut packet);
        (packet, mask)
    }

    fn temperature(packet: &[u8; PACKET_SIZE]) -> i16 {
        i16::from_be_bytes([packet[10], packet[11]])
    }
    fn humidity(packet: &[u8; PACKET_SIZE]) -> u16 {
        u16::from_be_bytes([packet[12], packet[13]])
    }
    fn brightness(packet: &[u8; PACKET_SIZE]) -> u32 {
        u32::from_be_bytes([packet[14], packet[15], packet[16], packet[17]])
    }
    fn ph(packet: &[u8; PACKET_SIZE]) -> u16 {
        u16::from_be_bytes([packet[18], packet[19]])
    }
    fn tds(packet: &[u8; PACKET_SIZE]) -> u16 {
        u16::from_be_bytes([packet[20], packet[21]])
    }
    fn atmosphere(packet: &[u8; PACKET_SIZE]) -> u16 {
        u16::from_be_bytes([packet[22], packet[23]])
    }
    fn soil(packet: &[u8; PACKET_SIZE]) -> u16 {
        u16::from_be_bytes([packet[24], packet[25]])
    }

    #[test]
    fn crc8_matches_the_reference_vector() {
        assert_eq!(calc_crc8(b"123456789"), 0xF4);
    }

    #[test]
    fn marks_present_sensors_and_writes_the_header() {
        let state = SensorState {
            temperature: Some(0.0),
            brightness: Some(98_000.0),
            atmosphere: Some(1013.2),
            ..Default::default()
        };
        let (packet, mask) = encode(&state, 42, 0);

        assert_eq!(mask, 0b0010_0101);
        assert_eq!(packet[0], COMPACT_BINARY_MAGIC);
        assert_eq!(packet[1], COMPACT_BINARY_VERSION);
        assert_eq!(u16::from_be_bytes([packet[2], packet[3]]), 42);
        assert_eq!(u16::from_be_bytes([packet[8], packet[9]]), mask);
        assert_eq!(temperature(&packet), 0);
        assert_eq!(brightness(&packet), 98_000);
        assert_eq!(
            calc_crc8(&packet[..PACKET_SIZE - 1]),
            packet[PACKET_SIZE - 1]
        );
    }

    #[test]
    fn rounds_half_lsb_away_from_zero_for_every_scaled_field() {
        let state = SensorState {
            temperature: Some(20.125),
            humidity: Some(49.125),
            brightness: Some(0.5),
            ph: Some(7.125),
            tds: Some(1234.5),
            atmosphere: Some(1013.25),
            soil_moisture: Some(12.125),
        };
        let (packet, mask) = encode(&state, 1, 0);

        assert_eq!(mask, 0b0111_1111);
        assert_eq!(temperature(&packet), 2013);
        assert_eq!(humidity(&packet), 4913);
        assert_eq!(brightness(&packet), 1);
        assert_eq!(ph(&packet), 713);
        assert_eq!(tds(&packet), 1235);
        assert_eq!(atmosphere(&packet), 10_133);
        assert_eq!(soil(&packet), 1213);

        let negative = SensorState {
            temperature: Some(-12.125),
            ..Default::default()
        };
        let (packet, mask) = encode(&negative, 1, 0);
        assert_eq!(mask, 1);
        assert_eq!(temperature(&packet), -1213);
    }

    #[test]
    fn out_of_range_values_clear_the_bit_and_leave_bytes_zero() {
        let state = SensorState {
            temperature: Some(60.01),
            humidity: Some(100.01),
            brightness: Some(200_001.0),
            ph: Some(14.01),
            tds: Some(10_001.0),
            atmosphere: Some(299.9),
            soil_moisture: Some(-0.01),
        };
        let (packet, mask) = encode(&state, 7, 0);

        assert_eq!(mask, 0);
        assert!(packet[10..26].iter().all(|byte| *byte == 0));
        assert_eq!(
            calc_crc8(&packet[..PACKET_SIZE - 1]),
            packet[PACKET_SIZE - 1]
        );
    }

    #[test]
    fn exact_api_boundaries_are_accepted() {
        let maximum = SensorState {
            temperature: Some(60.0),
            humidity: Some(100.0),
            brightness: Some(200_000.0),
            ph: Some(14.0),
            tds: Some(10_000.0),
            atmosphere: Some(1_100.0),
            soil_moisture: Some(100.0),
        };
        let (packet, mask) = encode(&maximum, 1, 0);
        assert_eq!(mask, 0b0111_1111);
        assert_eq!(temperature(&packet), 6_000);
        assert_eq!(humidity(&packet), 10_000);
        assert_eq!(brightness(&packet), 200_000);
        assert_eq!(ph(&packet), 1_400);
        assert_eq!(tds(&packet), 10_000);
        assert_eq!(atmosphere(&packet), 11_000);
        assert_eq!(soil(&packet), 10_000);

        let minimum = SensorState {
            temperature: Some(-20.0),
            humidity: Some(0.0),
            brightness: Some(0.0),
            ph: Some(0.0),
            tds: Some(0.0),
            atmosphere: Some(300.0),
            soil_moisture: Some(0.0),
        };
        let (packet, mask) = encode(&minimum, 1, 0);
        assert_eq!(mask, 0b0111_1111);
        assert_eq!(temperature(&packet), -2_000);
        assert_eq!(humidity(&packet), 0);
        assert_eq!(brightness(&packet), 0);
        assert_eq!(ph(&packet), 0);
        assert_eq!(tds(&packet), 0);
        assert_eq!(atmosphere(&packet), 3_000);
        assert_eq!(soil(&packet), 0);
    }

    #[test]
    fn non_finite_values_are_never_encoded() {
        let state = SensorState {
            temperature: Some(f32::NAN),
            humidity: Some(f32::INFINITY),
            brightness: Some(f32::NEG_INFINITY),
            ph: Some(f32::NAN),
            tds: Some(f32::INFINITY),
            atmosphere: Some(f32::NAN),
            soil_moisture: Some(f32::NEG_INFINITY),
        };
        let (packet, mask) = encode(&state, 0, 0);
        assert_eq!(mask, 0);
        assert!(packet[10..26].iter().all(|byte| *byte == 0));
    }

    #[test]
    fn writes_sequence_and_epoch_big_endian() {
        let (packet, _) = encode(&SensorState::default(), 0xABCD, 0x1234_5678);
        assert_eq!(u16::from_be_bytes([packet[2], packet[3]]), 0xABCD);
        assert_eq!(
            u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]),
            0x1234_5678
        );
    }

    #[test]
    fn a_value_just_outside_the_raw_range_that_rounds_inside_is_kept() {
        // 60.004 °C redondea a 6000 centésimas, dentro del rango de la API.
        let state = SensorState {
            temperature: Some(60.004),
            ..Default::default()
        };
        let (packet, mask) = encode(&state, 0, 0);
        assert_eq!(mask, 1);
        assert_eq!(temperature(&packet), 6_000);
    }
}
