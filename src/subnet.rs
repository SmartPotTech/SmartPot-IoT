//! Cálculo puro de la subred que debe incluir al router.
//!
//! smoltcp solo acepta respuestas ARP de direcciones de su misma subred. Si el
//! DHCP entrega una máscara que deja fuera al router (por ejemplo /32), el
//! ESP32 nunca aprende su MAC y ningún paquete sale de la red local, aunque el
//! DHCP haya funcionado. lwIP, la pila del firmware MicroPython, sí lo acepta.

/// Prefijo más largo admitido al ampliar: /30 deja dirección de red y de
/// difusión fuera de los dos hosts útiles.
const MAX_WIDENED_PREFIX: u8 = 30;

/// Si `gateway` queda fuera de `address/prefix_len`, devuelve el prefijo más
/// largo (como mucho /30) que contiene a ambos; si ya está dentro, `None`.
pub fn prefix_including_gateway(address: [u8; 4], prefix_len: u8, gateway: [u8; 4]) -> Option<u8> {
    let address = u32::from_be_bytes(address);
    let gateway = u32::from_be_bytes(gateway);
    let prefix_len = prefix_len.min(32);
    let mask = if prefix_len == 0 {
        0
    } else {
        u32::MAX << (32 - prefix_len)
    };
    if address & mask == gateway & mask {
        return None;
    }
    let common = (address ^ gateway).leading_zeros() as u8;
    Some(common.min(MAX_WIDENED_PREFIX))
}
