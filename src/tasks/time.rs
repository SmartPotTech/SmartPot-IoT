//! Cliente SNTP mínimo sobre UDP para fijar el epoch Unix del firmware.
//!
//! El handshake TLS necesita la hora para validar la vigencia de los
//! certificados, así que esta tarea arranca antes que el cliente MQTT. Espera a
//! que el stack tenga configuración IPv4, resuelve el servidor por DNS y repite
//! la sincronización cada 60 segundos (cada 10 hasta la primera, como el bucle
//! del firmware MicroPython). No usa heap: todos los búferes son
//! locales y de tamaño fijo.

use embassy_executor::task;
use embassy_futures::select::{Either, select};
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpAddress, IpEndpoint, Stack};
use embassy_time::{Duration, Instant, Timer};

use crate::clock;
use crate::sntp::{self, NTP_PACKET_SIZE};

const NTP_PORT: u16 = 123;
const SYNC_INTERVAL: Duration = Duration::from_secs(60);
/// Reintento hasta la primera sincronización: sin hora no hay TLS ni MQTT.
const FIRST_SYNC_RETRY: Duration = Duration::from_secs(10);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

#[task]
pub async fn sntp_task(stack: Stack<'static>, server: &'static str) {
    stack.wait_config_up().await;
    let mut request_counter: u64 = 0;
    let mut synchronized_once = false;

    loop {
        // Nonce que el servidor debe repetir: instante actual más un contador.
        request_counter = request_counter.wrapping_add(1);
        let nonce = (Instant::now().as_ticks() << 16) ^ request_counter;

        match sync_once(stack, server, nonce).await {
            Ok(epoch) => {
                clock::set_epoch(epoch);
                if !synchronized_once {
                    marker!(
                        "Clock synchronized: {}",
                        crate::report::localtime_tuple(epoch).as_str()
                    );
                    synchronized_once = true;
                }
                info!("[SNTP] Clock synchronized from {}: epoch {}", server, epoch);
            }
            Err(reason) => {
                warn!("[SNTP] Synchronization with {} failed: {}", server, reason);
                if !synchronized_once {
                    marker!("Could not synchronize clock: {}", reason);
                }
            }
        }

        Timer::after(if synchronized_once {
            SYNC_INTERVAL
        } else {
            FIRST_SYNC_RETRY
        })
        .await;
    }
}

/// Realiza una única petición SNTP y devuelve el epoch Unix del servidor.
async fn sync_once(stack: Stack<'static>, server: &str, nonce: u64) -> Result<u64, &'static str> {
    let [a, b, c, d] = crate::tasks::resolver::resolve_ipv4(stack, server)
        .await?
        .octets();
    let address = IpAddress::v4(a, b, c, d);

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

    let server_endpoint = IpEndpoint::new(address, NTP_PORT);
    socket
        .send_to(&sntp::build_request(nonce), server_endpoint)
        .await
        .map_err(|_| "could not send SNTP request")?;

    let mut response = [0u8; NTP_PACKET_SIZE];
    match select(
        socket.recv_from(&mut response),
        Timer::after(RESPONSE_TIMEOUT),
    )
    .await
    {
        Either::First(Ok((length, meta))) => {
            // Solo cuenta la respuesta del servidor consultado.
            if meta.endpoint != server_endpoint {
                return Err("SNTP response from an unexpected address");
            }
            sntp::parse_response(&response[..length], nonce)
        }
        Either::First(Err(_)) => Err("could not receive SNTP response"),
        Either::Second(_) => Err("SNTP request timed out"),
    }
}
