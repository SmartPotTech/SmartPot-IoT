//! Cliente DNS mínimo (solo registros A) para el resolvedor de respaldo.
//!
//! El DNS de smoltcp solo consulta el primer servidor que entrega el DHCP y
//! descarta las respuestas que no siguen su formato estricto. Esta lógica pura
//! construye la consulta y extrae la primera IPv4 de la respuesta, para poder
//! preguntar a otros servidores cuando aquel falla. No depende del hardware:
//! se prueba en el host.

use heapless::Vec;

/// Puerto UDP de DNS.
pub const DNS_PORT: u16 = 53;

/// Tamaño máximo de un mensaje DNS por UDP sin EDNS.
pub const MAX_MESSAGE_SIZE: usize = 512;

/// Cabecera (12), nombre en etiquetas (hasta 255), tipo y clase (4).
pub const QUERY_CAPACITY: usize = HEADER_SIZE + MAX_NAME_SIZE + 4;

const HEADER_SIZE: usize = 12;
const MAX_NAME_SIZE: usize = 255;
const MAX_LABEL_SIZE: usize = 63;
const TYPE_A: u16 = 1;
const CLASS_IN: u16 = 1;
/// Saltos máximos al seguir punteros de compresión (evita bucles).
const MAX_POINTERS: usize = 16;

/// Motivo por el que no se obtuvo una dirección.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsError {
    InvalidName,
    Malformed,
    UnexpectedId,
    NotAResponse,
    NameMismatch,
    NoSuchName,
    ServerFailure(u8),
    NoAddress,
    Timeout,
    Network,
}

impl DnsError {
    /// Texto en inglés para la consola.
    pub fn message(self) -> &'static str {
        match self {
            DnsError::InvalidName => "invalid host name",
            DnsError::Malformed => "malformed DNS response",
            DnsError::UnexpectedId => "unexpected DNS response id",
            DnsError::NotAResponse => "not a DNS response",
            DnsError::NameMismatch => "DNS response for another name",
            DnsError::NoSuchName => "host not found",
            DnsError::ServerFailure(_) => "DNS server failure",
            DnsError::NoAddress => "no IPv4 address returned",
            DnsError::Timeout => "DNS request timed out",
            DnsError::Network => "could not send DNS request",
        }
    }
}

/// Construye una consulta A recursiva para `name` con el identificador `id`.
pub fn build_query(id: u16, name: &str) -> Result<Vec<u8, QUERY_CAPACITY>, DnsError> {
    let name = name.strip_suffix('.').unwrap_or(name);
    if name.is_empty() {
        return Err(DnsError::InvalidName);
    }

    let mut query: Vec<u8, QUERY_CAPACITY> = Vec::new();
    let id = id.to_be_bytes();
    // Cabecera: id, RD = 1, una pregunta y ningún registro más.
    let header = [id[0], id[1], 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0];
    query
        .extend_from_slice(&header)
        .map_err(|_| DnsError::InvalidName)?;

    let mut encoded = 0usize;
    for label in name.split('.') {
        if label.is_empty() || label.len() > MAX_LABEL_SIZE {
            return Err(DnsError::InvalidName);
        }
        encoded += 1 + label.len();
        if encoded + 1 > MAX_NAME_SIZE {
            return Err(DnsError::InvalidName);
        }
        query
            .push(label.len() as u8)
            .map_err(|_| DnsError::InvalidName)?;
        query
            .extend_from_slice(label.as_bytes())
            .map_err(|_| DnsError::InvalidName)?;
    }
    let tail = [0, 0, TYPE_A as u8, 0, CLASS_IN as u8];
    query
        .extend_from_slice(&tail)
        .map_err(|_| DnsError::InvalidName)?;
    Ok(query)
}

/// Devuelve la primera IPv4 de una respuesta a la consulta `id` sobre `name`.
///
/// Admite punteros de compresión y cadenas CNAME: toma el primer registro A
/// de clase IN de la sección de respuestas.
pub fn parse_response(packet: &[u8], id: u16, name: &str) -> Result<[u8; 4], DnsError> {
    if packet.len() < HEADER_SIZE {
        return Err(DnsError::Malformed);
    }
    if read_u16(packet, 0)? != id {
        return Err(DnsError::UnexpectedId);
    }
    let flags = read_u16(packet, 2)?;
    // QR = 1 y código de operación QUERY (0).
    if flags & 0x8000 == 0 || (flags >> 11) & 0x0F != 0 {
        return Err(DnsError::NotAResponse);
    }
    match (flags & 0x000F) as u8 {
        0 => {}
        3 => return Err(DnsError::NoSuchName),
        code => return Err(DnsError::ServerFailure(code)),
    }

    let questions = read_u16(packet, 4)?;
    let answers = read_u16(packet, 6)?;
    if questions == 0 {
        return Err(DnsError::Malformed);
    }

    let mut offset = HEADER_SIZE;
    for index in 0..questions {
        let end = skip_name(packet, offset)?;
        if index == 0 && (!name_equals(packet, offset, name)? || read_u16(packet, end)? != TYPE_A) {
            return Err(DnsError::NameMismatch);
        }
        offset = end + 4;
        if offset > packet.len() {
            return Err(DnsError::Malformed);
        }
    }

    for _ in 0..answers {
        let end = skip_name(packet, offset)?;
        let record_type = read_u16(packet, end)?;
        let class = read_u16(packet, end + 2)?;
        let length = read_u16(packet, end + 8)? as usize;
        let start = end + 10;
        let data = packet
            .get(start..start + length)
            .ok_or(DnsError::Malformed)?;
        if record_type == TYPE_A && class == CLASS_IN && length == 4 {
            return Ok([data[0], data[1], data[2], data[3]]);
        }
        offset = start + length;
    }
    Err(DnsError::NoAddress)
}

fn read_u16(packet: &[u8], offset: usize) -> Result<u16, DnsError> {
    let bytes = packet.get(offset..offset + 2).ok_or(DnsError::Malformed)?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

/// Posición siguiente a un nombre (que puede acabar en un puntero).
fn skip_name(packet: &[u8], mut offset: usize) -> Result<usize, DnsError> {
    loop {
        let length = *packet.get(offset).ok_or(DnsError::Malformed)? as usize;
        match length & 0xC0 {
            0xC0 => {
                packet.get(offset + 1).ok_or(DnsError::Malformed)?;
                return Ok(offset + 2);
            }
            0x00 if length == 0 => return Ok(offset + 1),
            0x00 => offset += 1 + length,
            _ => return Err(DnsError::Malformed),
        }
    }
}

/// Compara sin distinguir mayúsculas el nombre en `offset` con `name`.
fn name_equals(packet: &[u8], mut offset: usize, name: &str) -> Result<bool, DnsError> {
    let name = name.strip_suffix('.').unwrap_or(name);
    let mut expected = name.split('.');
    let mut pointers = 0;
    loop {
        let length = *packet.get(offset).ok_or(DnsError::Malformed)? as usize;
        match length & 0xC0 {
            0xC0 => {
                let low = *packet.get(offset + 1).ok_or(DnsError::Malformed)? as usize;
                pointers += 1;
                if pointers > MAX_POINTERS {
                    return Err(DnsError::Malformed);
                }
                offset = ((length & 0x3F) << 8) | low;
            }
            0x00 if length == 0 => return Ok(expected.next().is_none()),
            0x00 => {
                let label = packet
                    .get(offset + 1..offset + 1 + length)
                    .ok_or(DnsError::Malformed)?;
                match expected.next() {
                    Some(part) if part.as_bytes().eq_ignore_ascii_case(label) => {}
                    _ => return Ok(false),
                }
                offset += 1 + length;
            }
            _ => return Err(DnsError::Malformed),
        }
    }
}
