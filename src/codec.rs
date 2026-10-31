//! Paquete compacto de telemetría v2 (27 bytes, big-endian): la alternativa binaria al JSON del contrato v1, en el
//! mismo tópico. La API acepta los dos; el firmware lo usa si se compila con `COMPACT_TELEMETRY`.
//!
//! - `[0]` 'P' (0x50) · `[1]` versión 2 · `[2..4]` secuencia u16 · `[4..8]` época u32 en s (0 = sin hora)
//! - `[8..10]` máscara de presencia u16: bit 0 temperatura, 1 humedad, 2 luz, 3 pH, 4 TDS, 5 presión, 6 sustrato
//! - `[10..12]` temperatura i16 ×100 °C · `[12..14]` humedad u16 ×100 % · `[14..18]` luz u32 lux
//! - `[18..20]` pH u16 ×100 · `[20..22]` TDS u16 ppm · `[22..24]` presión u16 ×10 hPa · `[24..26]` sustrato ×100 %
//! - `[26]` CRC-8 (polinomio 0x07) de los bytes 0..26
//!
//! Un sensor sin lectura deja su bit en 0 (como omitirlo en el JSON); un 0 real lleva su bit.

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
    if let Some(v) = finite(state.temperature) {
        mask |= 1 << 0;
        out[10..12].copy_from_slice(&((v * 100.0).clamp(-32768.0, 32767.0) as i16).to_be_bytes());
    }
    if let Some(v) = finite(state.humidity) {
        mask |= 1 << 1;
        out[12..14].copy_from_slice(&((v.clamp(0.0, 100.0) * 100.0) as u16).to_be_bytes());
    }
    if let Some(v) = finite(state.brightness) {
        mask |= 1 << 2;
        out[14..18].copy_from_slice(&(v.clamp(0.0, 4.0e9) as u32).to_be_bytes());
    }
    if let Some(v) = finite(state.ph) {
        mask |= 1 << 3;
        out[18..20].copy_from_slice(&((v.clamp(0.0, 14.0) * 100.0) as u16).to_be_bytes());
    }
    if let Some(v) = finite(state.tds) {
        mask |= 1 << 4;
        out[20..22].copy_from_slice(&(v.clamp(0.0, 65535.0) as u16).to_be_bytes());
    }
    if let Some(v) = finite(state.atmosphere) {
        mask |= 1 << 5;
        out[22..24].copy_from_slice(&((v.clamp(0.0, 6553.5) * 10.0) as u16).to_be_bytes());
    }
    if let Some(v) = finite(state.soil_moisture) {
        mask |= 1 << 6;
        out[24..26].copy_from_slice(&((v.clamp(0.0, 100.0) * 100.0) as u16).to_be_bytes());
    }
    out[8..10].copy_from_slice(&mask.to_be_bytes());
    out[PACKET_SIZE - 1] = calc_crc8(&out[..PACKET_SIZE - 1]);
    mask
}

fn finite(value: Option<f32>) -> Option<f32> {
    value.filter(|v| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_state_marks_present_sensors() {
        let state = SensorState {
            temperature: Some(0.0),
            brightness: Some(98_000.0),
            atmosphere: Some(1013.2),
            ..Default::default()
        };
        let mut packet = [0u8; PACKET_SIZE];
        let mask = encode_state(&state, 42, 0, &mut packet);

        assert_eq!(mask, 0b0010_0101);
        assert_eq!(packet[0], COMPACT_BINARY_MAGIC);
        assert_eq!(packet[1], COMPACT_BINARY_VERSION);
        assert_eq!(u16::from_be_bytes([packet[2], packet[3]]), 42);
        assert_eq!(u16::from_be_bytes([packet[8], packet[9]]), mask);
        assert_eq!(i16::from_be_bytes([packet[10], packet[11]]), 0);
        assert_eq!(
            u32::from_be_bytes([packet[14], packet[15], packet[16], packet[17]]),
            98_000
        );
        assert_eq!(
            calc_crc8(&packet[..PACKET_SIZE - 1]),
            packet[PACKET_SIZE - 1]
        );
    }
}
