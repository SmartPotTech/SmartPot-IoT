//! Configuración central del firmware SmartPot ESP32 en Rust.
//!
//! Admite valores predeterminados seguros para producción y sobreescrituras en
//! tiempo de compilación mediante variables de entorno (`option_env!`). El
//! broker usa TLS en el puerto 8883 salvo que `USE_TLS` lo desactive de forma
//! explícita; `BROKER_HOST` conserva `127.0.0.1` como valor cómodo para el
//! desarrollo local.

use heapless::Vec;

/// Configuración inmutable del firmware.
#[derive(Clone, Copy)]
pub struct Config {
    pub wifi_ssid: &'static str,
    pub wifi_password: &'static str,
    pub crop_id: &'static str,
    pub device_key: &'static str,
    pub broker_host: &'static str,
    pub broker_port: u16,
    pub use_tls: bool,
    /// CA propia del broker en formato PEM; vacía si no se configuró.
    pub ca_cert_pem: &'static str,
    /// Servidor SNTP usado para obtener el epoch Unix antes del handshake TLS.
    pub sntp_server: &'static str,
    pub telemetry_interval_secs: u64,
    /// Telemetría en el paquete compacto v2 (27 bytes) en vez de JSON; se activa definiendo COMPACT_TELEMETRY al
    /// compilar. La API acepta los dos formatos en el mismo tópico.
    pub compact_telemetry: bool,
}

pub const DEFAULT_CONFIG: Config = Config {
    wifi_ssid: match option_env!("SSID") {
        Some(v) => v,
        None => "Wokwi-GUEST",
    },
    wifi_password: match option_env!("PASSWORD") {
        Some(v) => v,
        None => "",
    },
    crop_id: match option_env!("CROP_ID") {
        Some(v) => v,
        None => "66f5a1000000000000000101",
    },
    device_key: match option_env!("DEVICE_KEY") {
        Some(v) => v,
        None => "demo-lettuce-device-key-2026",
    },
    broker_host: match option_env!("BROKER_HOST") {
        Some(v) => v,
        None => "127.0.0.1",
    },
    broker_port: match option_env!("BROKER_PORT") {
        Some(v) => parse_port(v),
        None => 8883,
    },
    use_tls: match option_env!("USE_TLS") {
        Some(v) => parse_tls_flag(v),
        None => true,
    },
    ca_cert_pem: match option_env!("CA_CERT_PEM") {
        Some(v) => v,
        None => "",
    },
    sntp_server: match option_env!("SNTP_SERVER") {
        Some(v) => v,
        None => "pool.ntp.org",
    },
    compact_telemetry: option_env!("COMPACT_TELEMETRY").is_some(),
    telemetry_interval_secs: match option_env!("TELEMETRY_INTERVAL_SECS") {
        Some(v) => parse_u64_or(v, 15),
        None => 15,
    },
};

/// Periodo de muestreo de los sensores; la simulación lo acorta al compilar.
pub const SENSOR_INTERVAL_SECS: u64 = match option_env!("SENSOR_INTERVAL_SECS") {
    Some(v) => parse_u64_or(v, crate::board::DEFAULT_SENSOR_INTERVAL_SECS),
    None => crate::board::DEFAULT_SENSOR_INTERVAL_SECS,
};

/// Convierte un entero decimal positivo; cualquier valor vacío, nulo o inválido
/// devuelve `default`.
pub const fn parse_u64_or(value: &str, default: u64) -> u64 {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return default;
    }

    let mut result: u64 = 0;
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        if !byte.is_ascii_digit() {
            return default;
        }
        result = match result.checked_mul(10) {
            Some(v) => match v.checked_add((byte - b'0') as u64) {
                Some(v) => v,
                None => return default,
            },
            None => return default,
        };
        i += 1;
    }

    if result == 0 { default } else { result }
}

/// Convierte un entero decimal con signo opcional; cualquier valor vacío, nulo o inválido
/// devuelve `default`.
pub const fn parse_i64_or(value: &str, default: i64) -> i64 {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return default;
    }
    let (is_neg, start) = if bytes[0] == b'-' {
        (true, 1)
    } else if bytes[0] == b'+' {
        (false, 1)
    } else {
        (false, 0)
    };
    if start >= bytes.len() {
        return default;
    }

    let mut result: i64 = 0;
    let mut i = start;
    while i < bytes.len() {
        let byte = bytes[i];
        if !byte.is_ascii_digit() {
            return default;
        }
        result = match result.checked_mul(10) {
            Some(v) => match v.checked_add((byte - b'0') as i64) {
                Some(v) => v,
                None => return default,
            },
            None => return default,
        };
        i += 1;
    }

    if is_neg { -result } else { result }
}

/// Decodifica la CA configurada en `CA_CERT_PEM` (PEM) a DER.
///
/// Solo se admite PEM con las cabeceras `BEGIN/END CERTIFICATE`. Si la variable
/// no está definida o el contenido es inválido se devuelve `Err(())`; el cliente
/// MQTT lo trata como un fallo cerrado y nunca cae a una conexión sin verificar.
#[allow(clippy::result_unit_err)]
pub fn ca_cert_der<const N: usize>(pem: &str) -> Result<Vec<u8, N>, ()> {
    if pem.trim().is_empty() {
        return Err(());
    }

    let mut der = Vec::new();
    pem_to_der(pem, &mut der)?;
    if der.is_empty() {
        return Err(());
    }
    Ok(der)
}

const PEM_BEGIN: &str = "-----BEGIN CERTIFICATE-----";
const PEM_END: &str = "-----END CERTIFICATE-----";

/// Extrae el cuerpo base64 de un PEM de certificado y lo decodifica a DER.
fn pem_to_der<const N: usize>(pem: &str, out: &mut Vec<u8, N>) -> Result<(), ()> {
    let body_start = pem.find(PEM_BEGIN).ok_or(())? + PEM_BEGIN.len();
    let body_end = pem[body_start..].find(PEM_END).ok_or(())? + body_start;
    base64_decode(&pem.as_bytes()[body_start..body_end], out)
}

/// Decodifica base64 estándar ignorando espacios en blanco y validando el relleno.
fn base64_decode<const N: usize>(input: &[u8], out: &mut Vec<u8, N>) -> Result<(), ()> {
    let mut accumulator: u32 = 0;
    let mut bits: u8 = 0;
    let mut padding = false;

    for &byte in input {
        match byte {
            b' ' | b'\t' | b'\r' | b'\n' => continue,
            b'=' => {
                padding = true;
                continue;
            }
            _ => {}
        }

        if padding {
            return Err(());
        }

        let value = base64_value(byte).ok_or(())?;
        accumulator = (accumulator << 6) | value as u32;
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            let decoded = ((accumulator >> bits) & 0xff) as u8;
            out.push(decoded).map_err(|_| ())?;
        }
    }

    // Los bits sobrantes del último grupo deben ser cero.
    if bits > 0 && (accumulator & ((1 << bits) - 1)) != 0 {
        return Err(());
    }

    Ok(())
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Convierte `BROKER_PORT` en un puerto válido; cualquier valor inválido usa 8883.
const fn parse_port(value: &str) -> u16 {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return 8883;
    }

    let mut port: u32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        if !byte.is_ascii_digit() {
            return 8883;
        }
        port = port * 10 + (byte - b'0') as u32;
        if port > u16::MAX as u32 {
            return 8883;
        }
        i += 1;
    }

    if port == 0 { 8883 } else { port as u16 }
}

/// Interpreta `USE_TLS`: `0`, `false` y `no` desactivan TLS; el resto lo activa.
const fn parse_tls_flag(value: &str) -> bool {
    let bytes = value.as_bytes();

    if bytes.len() == 1 && bytes[0] == b'0' {
        return false;
    }
    if bytes.len() == 5 && eq_ignore_ascii(bytes, b"false") {
        return false;
    }
    if bytes.len() == 2 && eq_ignore_ascii(bytes, b"no") {
        return false;
    }

    true
}

const fn eq_ignore_ascii(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let mut i = 0;
    while i < left.len() {
        let a = left[i].to_ascii_lowercase();
        let b = right[i].to_ascii_lowercase();
        if a != b {
            return false;
        }
        i += 1;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_known_base64_vector() {
        let mut der = Vec::<u8, 32>::new();
        base64_decode(b"AQIDBAUGBwg=", &mut der).unwrap();
        assert_eq!(
            der.as_slice(),
            &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]
        );
    }

    #[test]
    fn ignores_whitespace_inside_the_pem_body() {
        let pem =
            "-----BEGIN CERTIFICATE-----\r\nAQID\r\nBAUG\r\nBwg=\r\n-----END CERTIFICATE-----\r\n";
        let mut der = Vec::<u8, 32>::new();
        pem_to_der(pem, &mut der).unwrap();
        assert_eq!(
            der.as_slice(),
            &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]
        );
    }

    #[test]
    fn rejects_pem_without_headers_or_with_bad_base64() {
        let mut der = Vec::<u8, 32>::new();
        assert!(pem_to_der("AQIDBAUGBwg=", &mut der).is_err());

        let mut der = Vec::<u8, 32>::new();
        assert!(
            pem_to_der(
                "-----BEGIN CERTIFICATE-----\nnot*base64\n-----END CERTIFICATE-----",
                &mut der
            )
            .is_err()
        );
    }

    #[test]
    fn decodes_a_small_der_certificate_placeholder() {
        // Vector sintético pequeño: no es la CA real, solo valida el flujo PEM → DER.
        let mut der = Vec::<u8, 64>::new();
        base64_decode(b"MAkCAQECAwQFBgc=", &mut der).unwrap();
        assert_eq!(
            der.as_slice(),
            &[
                0x30, 0x09, 0x02, 0x01, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07
            ]
        );
    }

    #[test]
    fn parses_secure_tls_defaults() {
        assert!(parse_tls_flag("1"));
        assert!(parse_tls_flag("true"));
        assert!(parse_tls_flag("YES"));
        assert!(!parse_tls_flag("0"));
        assert!(!parse_tls_flag("false"));
        assert!(!parse_tls_flag("NO"));
        assert_eq!(parse_port("8883"), 8883);
        assert_eq!(parse_port("1883"), 1883);
        assert_eq!(parse_port("not-a-port"), 8883);
        assert_eq!(parse_port("70000"), 8883);
    }
}
