//! Lógica pura de SNTP: petición, validación de la respuesta y conciliación
//! del reloj. No depende del hardware para poder probarla en el host.
//!
//! La respuesta solo se acepta si su *originate timestamp* repite el nonce que
//! envió el firmware: así un datagrama ajeno o falsificado sin acceso a la
//! petición no puede mover el reloj del que depende la validez del TLS.

/// Tamaño de un paquete SNTP sin extensiones.
pub const NTP_PACKET_SIZE: usize = 48;

/// Diferencia entre el epoch NTP (1900) y el epoch Unix (1970), en segundos.
const NTP_UNIX_DELTA: u64 = 2_208_988_800;

/// Segundos de una era NTP (el contador de 32 bits se desborda en 2036).
const NTP_ERA_SECONDS: u64 = 1 << 32;

/// Ninguna hora anterior al 1 de enero de 2026 es válida para este firmware.
pub const MIN_VALID_EPOCH: u64 = 1_767_225_600;

/// Retraso máximo del servidor respecto al reloj local que se ignora: el
/// segundo truncado de la respuesta puede quedar hasta un segundo atrás.
const BACKWARD_TOLERANCE_SECS: u64 = 2;

/// Construye una petición cliente (LI = 0, VN = 4, modo 3) con el nonce en el
/// campo *transmit timestamp*.
pub fn build_request(nonce: u64) -> [u8; NTP_PACKET_SIZE] {
    let mut request = [0u8; NTP_PACKET_SIZE];
    request[0] = 0x23;
    request[40..48].copy_from_slice(&nonce.to_be_bytes());
    request
}

/// Valida una respuesta y devuelve el epoch Unix del servidor.
pub fn parse_response(packet: &[u8], nonce: u64) -> Result<u64, &'static str> {
    if packet.len() < NTP_PACKET_SIZE {
        return Err("short SNTP response");
    }

    let mode = packet[0] & 0x07;
    let version = (packet[0] >> 3) & 0x07;
    let stratum = packet[1];

    if mode != 4 {
        return Err("unexpected SNTP mode");
    }
    if !(3..=4).contains(&version) {
        return Err("unexpected SNTP version");
    }
    // Stratum 0 es un Kiss-o'-Death y 16..255 está reservado.
    if stratum == 0 || stratum > 15 {
        return Err("SNTP server is not synchronized");
    }
    if packet[24..32] != nonce.to_be_bytes() {
        return Err("SNTP response does not match the request");
    }

    let seconds = u32::from_be_bytes([packet[40], packet[41], packet[42], packet[43]]) as u64;
    // Los valores por debajo del desfase de 1970 pertenecen a la era 1 (2036+).
    let epoch = if seconds >= NTP_UNIX_DELTA {
        seconds - NTP_UNIX_DELTA
    } else {
        seconds + NTP_ERA_SECONDS - NTP_UNIX_DELTA
    };
    if epoch < MIN_VALID_EPOCH {
        return Err("invalid SNTP timestamp");
    }
    Ok(epoch)
}

/// Decide el epoch que se fija tras una respuesta SNTP válida.
///
/// Devuelve `None` si basta con conservar el reloj actual. Adelantos y valores
/// iguales siempre se aplican (renuevan la referencia); un retraso de hasta dos
/// segundos se ignora para no hacer retroceder la hora por el truncado, y uno
/// mayor corrige un cristal que adelanta.
pub fn reconcile_epoch(current: Option<u64>, server: u64) -> Option<u64> {
    match current {
        None => Some(server),
        Some(current) if server >= current => Some(server),
        Some(current) if current - server <= BACKWARD_TOLERANCE_SECS => None,
        Some(_) => Some(server),
    }
}
