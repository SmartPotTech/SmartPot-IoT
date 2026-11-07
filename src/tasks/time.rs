//! Cliente SNTP mínimo sobre UDP para fijar el epoch Unix del firmware.
//!
//! El handshake TLS necesita la hora para validar la vigencia de los
//! certificados, así que esta tarea arranca antes que el cliente MQTT. Espera a
//! que el stack tenga configuración IPv4, resuelve el servidor por DNS y repite
//! la sincronización cada 60 segundos. No usa heap: todos los búferes son
//! locales y de tamaño fijo.

use embassy_executor::task;
use embassy_futures::select::{Either, select};
use embassy_net::Stack;
use embassy_net::dns::DnsQueryType;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_time::{Duration, Timer};

use crate::clock;

const NTP_PACKET_SIZE: usize = 48;
const NTP_PORT: u16 = 123;
/// Diferencia entre el epoch NTP (1900) y el epoch Unix (1970), en segundos.
const NTP_UNIX_DELTA: u64 = 2_208_988_800;
const SYNC_INTERVAL: Duration = Duration::from_secs(60);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

#[task]
pub async fn sntp_task(stack: Stack<'static>, server: &'static str) {
    stack.wait_config_up().await;

    loop {
        match sync_once(stack, server).await {
            Ok(epoch) => {
                clock::set_epoch(epoch);
                info!("[SNTP] Clock synchronized from {}: epoch {}", server, epoch);
            }
            Err(reason) => {
                warn!("[SNTP] Synchronization with {} failed: {}", server, reason);
            }
        }

        Timer::after(SYNC_INTERVAL).await;
    }
}

/// Realiza una única petición SNTP y devuelve el epoch Unix del servidor.
async fn sync_once(stack: Stack<'static>, server: &str) -> Result<u64, &'static str> {
    let addresses = stack
        .dns_query(server, DnsQueryType::A)
        .await
        .map_err(|_| "DNS resolution failed")?;
    let address = *addresses.first().ok_or("no IPv4 address returned")?;

    let mut rx_meta = [PacketMetadata::EMPTY; 1];
    let mut tx_meta = [PacketMetadata::EMPTY; 1];
    let mut rx_buffer = [0u8; NTP_PACKET_SIZE];
    let mut tx_buffer = [0u8; NTP_PACKET_SIZE];
    let mut socket = UdpSocket::new(
        stack,
        &mut rx_meta,
        &mut rx_buffer,
        &mut tx_meta,
        &mut tx_buffer,
    );
    socket.bind(0).map_err(|_| "could not bind UDP socket")?;

    let mut request = [0u8; NTP_PACKET_SIZE];
    // LI = 0, VN = 4, Mode = 3 (client).
    request[0] = 0x23;

    socket
        .send_to(&request, (address, NTP_PORT))
        .await
        .map_err(|_| "could not send SNTP request")?;

    let mut response = [0u8; NTP_PACKET_SIZE];
    match select(
        socket.recv_from(&mut response),
        Timer::after(RESPONSE_TIMEOUT),
    )
    .await
    {
        Either::First(Ok((length, _))) => parse_response(&response[..length]),
        Either::First(Err(_)) => Err("could not receive SNTP response"),
        Either::Second(_) => Err("SNTP request timed out"),
    }
}

/// Valida la respuesta SNTP y extrae el timestamp de transmisión (bytes 40..44).
fn parse_response(packet: &[u8]) -> Result<u64, &'static str> {
    if packet.len() < NTP_PACKET_SIZE {
        return Err("short SNTP response");
    }

    let mode = packet[0] & 0x07;
    let version = (packet[0] >> 3) & 0x07;
    let stratum = packet[1];

    if mode != 4 {
        return Err("unexpected SNTP mode");
    }
    if version != 4 {
        return Err("unexpected SNTP version");
    }
    // Stratum 0 es un Kiss-o'-Death y 16..255 está reservado.
    if stratum == 0 || stratum > 15 {
        return Err("SNTP server is not synchronized");
    }

    let seconds = u32::from_be_bytes([packet[40], packet[41], packet[42], packet[43]]) as u64;
    if seconds < NTP_UNIX_DELTA {
        return Err("invalid SNTP timestamp");
    }

    Ok(seconds - NTP_UNIX_DELTA)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response_with(epoch: u32) -> [u8; NTP_PACKET_SIZE] {
        let mut packet = [0u8; NTP_PACKET_SIZE];
        packet[0] = 0x24; // LI = 0, VN = 4, Mode = 4 (server).
        packet[1] = 1; // Stratum primario.
        packet[40..44].copy_from_slice(&(epoch + NTP_UNIX_DELTA as u32).to_be_bytes());
        packet
    }

    #[test]
    fn parses_a_valid_server_response() {
        let packet = response_with(1_700_000_000);
        assert_eq!(parse_response(&packet), Ok(1_700_000_000));
    }

    #[test]
    fn rejects_invalid_mode_version_and_stratum() {
        let mut packet = response_with(1_700_000_000);
        packet[0] = 0x23; // Modo cliente, no servidor.
        assert!(parse_response(&packet).is_err());

        let mut packet = response_with(1_700_000_000);
        packet[0] = 0x1c; // Versión 3.
        assert!(parse_response(&packet).is_err());

        let mut packet = response_with(1_700_000_000);
        packet[1] = 0;
        assert!(parse_response(&packet).is_err());

        let mut packet = response_with(1_700_000_000);
        packet[1] = 16;
        assert!(parse_response(&packet).is_err());
    }

    #[test]
    fn rejects_short_or_pre_epoch_packets() {
        assert!(parse_response(&[0u8; 12]).is_err());

        let mut packet = response_with(0);
        packet[40..44].copy_from_slice(&0u32.to_be_bytes());
        assert!(parse_response(&packet).is_err());
    }
}
