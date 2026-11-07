//! Resolución de nombres con respaldo, compartida por SNTP y MQTT.
//!
//! Primero usa el DNS del stack (smoltcp). Si falla, pregunta directamente a
//! los servidores DNS del DHCP, al gateway y a dos resolvedores públicos. El
//! DNS de smoltcp solo consulta el primer servidor del DHCP y es estricto con
//! el formato de la respuesta, y algunos gateways (como el público de Wokwi)
//! no encajan con eso aunque la red funcione.

use core::net::Ipv4Addr;

use embassy_futures::select::{Either, select};
use embassy_net::dns::DnsQueryType;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpAddress, IpEndpoint, Stack};
use embassy_time::{Duration, Instant, Timer};
use heapless::Vec;

use crate::dns::{self, DNS_PORT, DnsError, MAX_MESSAGE_SIZE, QUERY_CAPACITY};

/// Resolvedores públicos que se prueban al final.
const PUBLIC_RESOLVERS: [Ipv4Addr; 2] = [Ipv4Addr::new(8, 8, 8, 8), Ipv4Addr::new(1, 1, 1, 1)];

/// Espera máxima por servidor en el respaldo.
const QUERY_TIMEOUT: Duration = Duration::from_secs(3);

/// Texto de consola cuando ningún servidor resuelve el nombre.
pub const RESOLUTION_FAILED: &str = "DNS resolution failed";

/// Resuelve `host` a IPv4. Acepta también literales IPv4.
pub async fn resolve_ipv4(stack: Stack<'static>, host: &str) -> Result<Ipv4Addr, &'static str> {
    if let Ok(address) = host.parse::<Ipv4Addr>() {
        return Ok(address);
    }

    match stack.dns_query(host, DnsQueryType::A).await {
        Ok(addresses) => {
            if let Some(address) = addresses.iter().find_map(ipv4) {
                return Ok(address);
            }
        }
        Err(e) => info!("[DNS] Stack lookup of {} failed: {:?}", host, e),
    }

    let mut not_found = false;
    for server in fallback_servers(stack) {
        match query(stack, server, host).await {
            Ok(address) => {
                info!("[DNS] {} resolved by {}: {}", host, server, address);
                return Ok(address);
            }
            Err(error) => {
                info!("[DNS] {} via {} failed: {}", host, server, error.message());
                not_found |= error == DnsError::NoSuchName;
            }
        }
    }
    Err(if not_found {
        DnsError::NoSuchName.message()
    } else {
        RESOLUTION_FAILED
    })
}

#[allow(unreachable_patterns)]
fn ipv4(address: &IpAddress) -> Option<Ipv4Addr> {
    match address {
        IpAddress::Ipv4(ipv4) => Some(Ipv4Addr::from(ipv4.octets())),
        // La variante IPv6 solo existe si se compila proto-ipv6.
        _ => None,
    }
}

/// Servidores del DHCP, el gateway y los resolvedores públicos, sin repetir.
fn fallback_servers(stack: Stack<'static>) -> Vec<Ipv4Addr, 6> {
    let mut servers: Vec<Ipv4Addr, 6> = Vec::new();
    let mut add = |server: Ipv4Addr| {
        if !server.is_unspecified() && !servers.contains(&server) {
            let _ = servers.push(server);
        }
    };
    if let Some(config) = stack.config_v4() {
        for server in config.dns_servers.iter() {
            add(Ipv4Addr::from(server.octets()));
        }
        if let Some(gateway) = config.gateway {
            add(Ipv4Addr::from(gateway.octets()));
        }
    }
    for server in PUBLIC_RESOLVERS {
        add(server);
    }
    servers
}

/// Una consulta A a `server`; solo acepta la respuesta de ese servidor.
async fn query(stack: Stack<'static>, server: Ipv4Addr, host: &str) -> Result<Ipv4Addr, DnsError> {
    let id = esp_hal::rng::Rng::new().random() as u16;
    let request = dns::build_query(id, host)?;

    let mut rx_meta = [PacketMetadata::EMPTY; 2];
    let mut tx_meta = [PacketMetadata::EMPTY; 1];
    let mut rx_buffer = [0u8; MAX_MESSAGE_SIZE];
    let mut tx_buffer = [0u8; QUERY_CAPACITY];
    let mut socket = UdpSocket::new(
        stack,
        &mut rx_meta,
        &mut rx_buffer,
        &mut tx_meta,
        &mut tx_buffer,
    );
    socket.bind(0).map_err(|_| DnsError::Network)?;

    let [a, b, c, d] = server.octets();
    let endpoint = IpEndpoint::new(IpAddress::v4(a, b, c, d), DNS_PORT);
    socket
        .send_to(&request, endpoint)
        .await
        .map_err(|_| DnsError::Network)?;

    let deadline = Instant::now() + QUERY_TIMEOUT;
    let mut response = [0u8; MAX_MESSAGE_SIZE];
    loop {
        match select(socket.recv_from(&mut response), Timer::at(deadline)).await {
            Either::First(Ok((length, meta))) => {
                if meta.endpoint != endpoint {
                    continue;
                }
                match dns::parse_response(&response[..length], id, host) {
                    Ok(octets) => return Ok(Ipv4Addr::from(octets)),
                    // Respuesta tardía de una consulta anterior: se sigue esperando.
                    Err(DnsError::UnexpectedId) => continue,
                    Err(error) => return Err(error),
                }
            }
            Either::First(Err(_)) => return Err(DnsError::Malformed),
            Either::Second(_) => return Err(DnsError::Timeout),
        }
    }
}
