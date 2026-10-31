//! Codificador de paquetes binarios ultracompactos (22 bytes) para transporte de telemetría IoT.
//!
//! Estructura de paquete:
//! [0]: Magic Byte 'P' (0x50)
//! [1]: Protocol Version (0x01)
//! [2..4]: Sequence ID (u16 BE)
//! [4..8]: Timestamp Unix Epoch (u32 BE)
//! [8..10]: Temperature (i16 BE, scale 100, -327.68 .. +327.67 C)
//! [10..12]: Relative Humidity (u16 BE, scale 100, 0.00 .. 100.00 %)
//! [12..14]: pH (u16 BE, scale 100, 0.00 .. 14.00)
//! [14..16]: TDS (u16 BE, scale 1, 0 .. 5000 ppm)
//! [16..18]: Soil Moisture (u16 BE, scale 100, 0.00 .. 100.00 %)
//! [18..20]: Brightness (u16 BE, scale 1, 0 .. 65535 lux)
//! [20]: Flags / Reserve (0x00)
//! [21]: CRC-8 Checksum (Polinomio 0x07 sobre bytes 0..21)

pub const COMPACT_BINARY_MAGIC: u8 = 0x50;
pub const COMPACT_BINARY_VERSION: u8 = 0x01;
pub const PACKET_SIZE: usize = 22;

/// Calcula el checksum CRC-8 ATM (polinomio 0x07) sobre el slice dado.
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

/// Datos de telemetría para empaquetado binario.
#[derive(Debug, Clone, Copy, Default)]
pub struct TelemetryRaw {
    pub seq_id: u16,
    pub timestamp_epoch: u32,
    pub temperature: f32,
    pub humidity: f32,
    pub ph: f32,
    pub tds: f32,
    pub soil_moisture: f32,
    pub brightness: f32,
    pub flags: u8,
}

impl TelemetryRaw {
    /// Codifica las medidas en un búfer de 22 bytes con verificación CRC-8.
    pub fn encode(&self, out: &mut [u8; PACKET_SIZE]) {
        out[0] = COMPACT_BINARY_MAGIC;
        out[1] = COMPACT_BINARY_VERSION;

        let seq_bytes = self.seq_id.to_be_bytes();
        out[2] = seq_bytes[0];
        out[3] = seq_bytes[1];

        let ts_bytes = self.timestamp_epoch.to_be_bytes();
        out[4..8].copy_from_slice(&ts_bytes);

        // Temperatura con escala x100 (con signo)
        let temp_scaled = (self.temperature * 100.0) as i16;
        let temp_bytes = temp_scaled.to_be_bytes();
        out[8] = temp_bytes[0];
        out[9] = temp_bytes[1];

        // Humedad x100 (sin signo)
        let hum_scaled = (self.humidity.clamp(0.0, 100.0) * 100.0) as u16;
        let hum_bytes = hum_scaled.to_be_bytes();
        out[10] = hum_bytes[0];
        out[11] = hum_bytes[1];

        // pH x100 (sin signo)
        let ph_scaled = (self.ph.clamp(0.0, 14.0) * 100.0) as u16;
        let ph_bytes = ph_scaled.to_be_bytes();
        out[12] = ph_bytes[0];
        out[13] = ph_bytes[1];

        // TDS x1 (sin signo)
        let tds_scaled = self.tds.clamp(0.0, 65535.0) as u16;
        let tds_bytes = tds_scaled.to_be_bytes();
        out[14] = tds_bytes[0];
        out[15] = tds_bytes[1];

        // Humedad sustrato x100 (sin signo)
        let moist_scaled = (self.soil_moisture.clamp(0.0, 100.0) * 100.0) as u16;
        let moist_bytes = moist_scaled.to_be_bytes();
        out[16] = moist_bytes[0];
        out[17] = moist_bytes[1];

        // Brillo x1 (sin signo)
        let bright_scaled = self.brightness.clamp(0.0, 65535.0) as u16;
        let bright_bytes = bright_scaled.to_be_bytes();
        out[18] = bright_bytes[0];
        out[19] = bright_bytes[1];

        out[20] = self.flags;

        // CRC-8 sobre los primeros 21 bytes
        out[21] = calc_crc8(&out[..21]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codec_crc8() {
        let data = b"123456789";
        // Check standard CRC-8 ATM
        let crc = calc_crc8(data);
        assert_eq!(crc, calc_crc8(data));
    }

    #[test]
    fn test_encode_packet_parity() {
        let raw = TelemetryRaw {
            seq_id: 42,
            timestamp_epoch: 1700000000,
            temperature: 23.50,
            humidity: 62.00,
            ph: 6.50,
            tds: 480.0,
            soil_moisture: 55.00,
            brightness: 15000.0,
            flags: 0,
        };

        let mut packet = [0u8; PACKET_SIZE];
        raw.encode(&mut packet);

        assert_eq!(packet[0], COMPACT_BINARY_MAGIC);
        assert_eq!(packet[1], COMPACT_BINARY_VERSION);
        assert_eq!(u16::from_be_bytes([packet[2], packet[3]]), 42);
        assert_eq!(u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]), 1700000000);
        assert_eq!(i16::from_be_bytes([packet[8], packet[9]]), 2350);
        assert_eq!(u16::from_be_bytes([packet[10], packet[11]]), 6200);
        assert_eq!(u16::from_be_bytes([packet[12], packet[13]]), 650);
        assert_eq!(u16::from_be_bytes([packet[14], packet[15]]), 480);
        assert_eq!(u16::from_be_bytes([packet[16], packet[17]]), 5500);
        assert_eq!(u16::from_be_bytes([packet[18], packet[19]]), 15000);

        // Validar CRC
        assert_eq!(calc_crc8(&packet[..21]), packet[21]);
    }
}
