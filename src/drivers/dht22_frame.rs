//! Decodificación de la trama de 40 bits del DHT22 (AM2302).
//!
//! Es pura para poder probarla en el host; la lectura del bus está en `dht22.rs`.

/// Devuelve `(temperatura °C, humedad %)` o `None` si la suma de control falla.
///
/// Trama: humedad (16 bits, décimas), temperatura (15 bits, décimas, el bit
/// alto es el signo) y suma de control de los cuatro bytes anteriores.
pub fn decode(bytes: [u8; 5]) -> Option<(f32, f32)> {
    let sum = bytes[..4]
        .iter()
        .fold(0u8, |acc, byte| acc.wrapping_add(*byte));
    if sum != bytes[4] {
        return None;
    }
    let humidity = u16::from_be_bytes([bytes[0], bytes[1]]) as f32 / 10.0;
    let magnitude = u16::from_be_bytes([bytes[2] & 0x7F, bytes[3]]) as f32 / 10.0;
    let temperature = if bytes[2] & 0x80 != 0 {
        -magnitude
    } else {
        magnitude
    };
    Some((temperature, humidity))
}
