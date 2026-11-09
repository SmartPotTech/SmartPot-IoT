//! Cliente MQTT robusto con soporte de contrato v1 (telemetría, comandos, confirmación ACK y LWT).
//!
//! La conexión usa TLS 1.3 con verificación de la CA propia del broker cuando
//! `USE_TLS` está activo (valor por defecto). El epoch Unix se obtiene por SNTP
//! antes del handshake y nunca se cae a una conexión sin verificar: si falta la
//! CA o el reloj, el cliente registra el error y reintenta.
//!
//! La sesión detecta enlaces muertos por tres vías: PINGREQ cada 30 s con plazo
//! para el PINGRESP, tiempo límite del socket TCP y keepalive TCP. Los
//! reintentos usan espera exponencial con dispersión aleatoria.

use core::fmt::Write as _;
use core::net::{Ipv4Addr, SocketAddrV4};
use core::num::NonZero;
use core::str;

use embassy_executor::task;
use embassy_futures::select::{Either, Either4, select, select4};
use embassy_net::Stack;
use embassy_net::tcp::TcpSocket as NetTcpSocket;
use embassy_sync::pubsub::{DynPublisher, DynSubscriber};
use embassy_time::{Duration, Instant, Ticker, Timer, with_timeout};

use embedded_tls::{TlsConfig, TlsConnection, TlsContext};
use esp_hal::rng::Trng;
use heapless::String;
use rust_mqtt::buffer::AllocBuffer;
use rust_mqtt::client::Client;
use rust_mqtt::client::MqttError;
use rust_mqtt::client::event::Event;
use rust_mqtt::client::options::{
    ConnectOptions, PublicationOptions, SubscriptionOptions, TopicReference, WillOptions,
};
use rust_mqtt::config::KeepAlive;
use rust_mqtt::types::{MqttBinary, MqttString, ReasonCode, TopicName};
use static_cell::ConstStaticCell;

use crate::backoff::Backoff;
use crate::capabilities::CAPABILITIES_JSON;
use crate::clock;
use crate::config::{self, Config};
use crate::events::telemetry::TelemetryMessage;
use crate::events::{AckMessage, BROKER_ONLINE, CA_REJECTED, Command};
use crate::health::{Beat, beat};
use crate::protocol::{TopicResolver, parse_command};
use crate::tls::VerifyingProvider;

/// Tamaño del búfer para la cadena de certificados del servidor durante el handshake.
const TLS_CERT_BUFFER_SIZE: usize = 2048;

/// Intervalo de PINGREQ y plazo máximo para recibir el PINGRESP.
const PING_INTERVAL: Duration = Duration::from_secs(30);
const PING_DEADLINE: Duration = Duration::from_secs(15);

/// Sin datos ni ACK del broker durante este tiempo, el socket TCP se aborta.
const SOCKET_TIMEOUT: Duration = Duration::from_secs(90);
const SOCKET_KEEP_ALIVE: Duration = Duration::from_secs(30);

/// Paquete más grande que el firmware acepta del broker. Un comando ocupa
/// menos de 200 bytes; el límite evita reservas enormes en un heap de 98 KB.
const MAX_INCOMING_PACKET: u32 = 1024;

/// Tiempo máximo para despedirse del broker al cerrar una sesión rota.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

/// Espera entre reintentos: de 2 s a 60 s con dispersión.
const RETRY_BASE_MS: u32 = 2_000;
const RETRY_MAX_MS: u32 = 60_000;

/// Tópicos del cultivo, resueltos una sola vez.
struct Topics {
    telemetry: String<64>,
    commands: String<64>,
    ack: String<64>,
    status: String<64>,
    capabilities: String<64>,
}

/// Resultado de una sesión MQTT, para decidir la espera del siguiente intento.
enum SessionOutcome {
    /// El broker rechazó la conexión o la suscripción.
    Refused,
    /// La sesión llegó a estar operativa y después se cerró.
    Ended,
}

/// Convierte un tópico en `TopicName`; falla si el cultivo está mal configurado.
fn topic_name(topic: &str) -> Option<TopicName<'_>> {
    TopicName::new(MqttString::from_str(topic).ok()?)
}

/// Espera a que el stack tenga IPv4 sin dejar de latir para el supervisor.
async fn wait_for_network(stack: Stack<'static>) {
    loop {
        beat(Beat::Mqtt);
        if let Either::First(()) =
            select(stack.wait_config_up(), Timer::after(Duration::from_secs(5))).await
        {
            return;
        }
    }
}

/// Espera latiendo para que el supervisor no confunda un reintento con un bloqueo.
async fn sleep_with_beats(delay: Duration) {
    let deadline = Instant::now() + delay;
    while Instant::now() < deadline {
        beat(Beat::Mqtt);
        let remaining = deadline - Instant::now();
        Timer::after(remaining.min(Duration::from_secs(5))).await;
    }
}

#[task]
pub async fn mqtt_task(
    stack: Stack<'static>,
    rng: Trng,
    config: Config,
    mut telemetry_channel: DynSubscriber<'static, TelemetryMessage>,
    commands_channel: DynPublisher<'static, Command>,
    mut ack_channel: DynSubscriber<'static, AckMessage>,
) {
    static RX_BUFFER: ConstStaticCell<[u8; 4096]> = ConstStaticCell::new([0u8; 4096]);
    static TX_BUFFER: ConstStaticCell<[u8; 4096]> = ConstStaticCell::new([0u8; 4096]);
    static TLS_RX: ConstStaticCell<[u8; 16640]> = ConstStaticCell::new([0u8; 16640]);
    static TLS_TX: ConstStaticCell<[u8; 16640]> = ConstStaticCell::new([0u8; 16640]);

    let rx_buffer = RX_BUFFER.take();
    let tx_buffer = TX_BUFFER.take();
    let tls_rx = TLS_RX.take();
    let tls_tx = TLS_TX.take();

    let resolver = TopicResolver::new(config.crop_id);
    let topics = Topics {
        telemetry: resolver.telemetry(),
        commands: resolver.commands(),
        ack: resolver.ack(),
        status: resolver.status(),
        capabilities: resolver.capabilities(),
    };
    let mut backoff = Backoff::new(RETRY_BASE_MS, RETRY_MAX_MS);
    let mut ca_missing_logged = false;

    wait_for_network(stack).await;
    if let Some(net_cfg) = stack.config_v4() {
        marker!("WiFi connected: {}", net_cfg.address.address());
    }

    loop {
        beat(Beat::Mqtt);
        if let Some(net_cfg) = stack.config_v4() {
            info!(
                "[MQTT] Local IP: {}, Gateway: {:?}",
                net_cfg.address, net_cfg.gateway
            );
        }

        let outcome = 'attempt: {
            // Precondiciones de TLS: hora válida y CA propia. Se comprueban antes
            // de abrir el socket para no mantener conexiones TCP sin verificar.
            let ca_der = if config.use_tls {
                while !clock::is_synced() {
                    warn!("[MQTT] Waiting for SNTP time synchronization before TLS");
                    sleep_with_beats(Duration::from_secs(2)).await;
                }

                match config::ca_cert_der::<TLS_CERT_BUFFER_SIZE>(config.ca_cert_pem) {
                    Ok(der) => der,
                    Err(_) => {
                        if !ca_missing_logged {
                            // Mismos textos que el firmware MicroPython.
                            if config.ca_cert_pem.trim().is_empty() {
                                marker!(
                                    "Cannot connect securely: config.json is missing the broker CA: copy the BROKER block from the connection guide"
                                );
                            } else {
                                marker!(
                                    "Cannot connect securely: the CA in config.json is corrupted: copy the BROKER block again"
                                );
                            }
                            CA_REJECTED.sender().send(true);
                            error!(
                                "[MQTT] CA_CERT_PEM is missing or invalid; refusing to connect without certificate verification"
                            );
                            ca_missing_logged = true;
                        }
                        break 'attempt SessionOutcome::Refused;
                    }
                }
            } else {
                heapless::Vec::new()
            };

            info!(
                "[MQTT] Connecting to {}:{} (Crop: {})",
                config.broker_host, config.broker_port, config.crop_id
            );

            let Some(broker_ip) = resolve_broker(stack, config.broker_host).await else {
                marker!(
                    "No connection to broker: could not resolve {}",
                    config.broker_host
                );
                break 'attempt SessionOutcome::Refused;
            };
            let broker_addr = SocketAddrV4::new(broker_ip, config.broker_port);

            let mut socket = NetTcpSocket::new(stack, rx_buffer, tx_buffer);
            socket.set_timeout(Some(SOCKET_TIMEOUT));
            socket.set_keep_alive(Some(SOCKET_KEEP_ALIVE));
            if let Err(e) = socket.connect(broker_addr).await {
                error!("[MQTT] TCP connection failed: {:?}", e);
                marker!("No connection to broker: {:?}", e);
                break 'attempt SessionOutcome::Refused;
            }
            info!("[MQTT] TCP connection established");
            beat(Beat::Mqtt);

            if config.use_tls {
                let tls_config = TlsConfig::new()
                    .with_server_name(config.broker_host)
                    .enable_rsa_signatures();
                let provider = VerifyingProvider::new(rng.clone(), ca_der.as_slice());
                let tls_context = TlsContext::new(&tls_config, provider);
                let mut tls = TlsConnection::new(socket, tls_rx, tls_tx);
                if let Err(e) = tls.open(tls_context).await {
                    error!("[MQTT] TLS handshake failed: {:?}", e);
                    marker!(
                        "No connection to broker: could not verify the broker ({:?}): check the CA in config.json and the system clock",
                        e
                    );
                    break 'attempt SessionOutcome::Refused;
                }
                info!("[MQTT] TLS session established; broker certificate verified");
                marker!("SMARTPOT-TLS-OK host={}", config.broker_host);

                run_mqtt_client(
                    &mut tls,
                    &config,
                    &topics,
                    &mut telemetry_channel,
                    &commands_channel,
                    &mut ack_channel,
                )
                .await
            } else {
                warn!("[MQTT] TLS is disabled; connecting without certificate verification");

                run_mqtt_client(
                    &mut socket,
                    &config,
                    &topics,
                    &mut telemetry_channel,
                    &commands_channel,
                    &mut ack_channel,
                )
                .await
            }
        };

        if let SessionOutcome::Ended = outcome {
            backoff.reset();
        }
        let delay = backoff.next_delay_ms(rng.random());
        warn!(
            "[MQTT] Connection closed. Retrying in {} ms (attempt {})",
            delay,
            backoff.attempt()
        );
        sleep_with_beats(Duration::from_millis(delay as u64)).await;
    }
}

/// Resuelve el host del broker a IPv4 (DNS del stack con respaldo). Acepta
/// también literales IP, así que `BROKER_HOST=127.0.0.1` sigue funcionando.
async fn resolve_broker(stack: Stack<'static>, host: &str) -> Option<Ipv4Addr> {
    match crate::tasks::resolver::resolve_ipv4(stack, host).await {
        Ok(address) => Some(address),
        Err(reason) => {
            error!("[MQTT] Could not resolve {}: {}", host, reason);
            None
        }
    }
}

/// Convierte un mensaje de telemetría en el estado que se publica.
fn telemetry_state(message: TelemetryMessage) -> crate::protocol::SensorState {
    match message {
        TelemetryMessage::SendAll(state) => state,
        TelemetryMessage::SendOne(measurement) => {
            let mut state = crate::protocol::SensorState::default();
            match measurement {
                crate::events::Measurements::BME280((t, h, p)) => state.update_bme280(t, h, p),
                crate::events::Measurements::Air((t, h)) => {
                    state.temperature = Some(t);
                    state.humidity = Some(h);
                }
                crate::events::Measurements::BH1750(l) => state.update_lux(l),
                crate::events::Measurements::PH(p) => state.update_ph(p),
                crate::events::Measurements::TDS(t) => state.update_tds(t),
                crate::events::Measurements::SoilMoisture(sm) => state.update_soil_moisture(sm),
            }
            state
        }
    }
}

async fn run_mqtt_client<T>(
    transport: &mut T,
    config: &Config,
    topics: &Topics,
    telemetry_channel: &mut DynSubscriber<'static, TelemetryMessage>,
    commands_channel: &DynPublisher<'static, Command>,
    ack_channel: &mut DynSubscriber<'static, AckMessage>,
) -> SessionOutcome
where
    T: embedded_io_async::Read + embedded_io_async::Write,
{
    let (
        Some(status_topic),
        Some(capabilities_topic),
        Some(commands_topic),
        Some(ack_topic),
        Some(telemetry_topic),
    ) = (
        topic_name(&topics.status),
        topic_name(&topics.capabilities),
        topic_name(&topics.commands),
        topic_name(&topics.ack),
        topic_name(&topics.telemetry),
    )
    else {
        error!("[MQTT] Invalid MQTT topics; check CROP_ID");
        return SessionOutcome::Refused;
    };
    let (Ok(user_name), Ok(password), Ok(offline)) = (
        MqttString::from_str(config.crop_id),
        MqttBinary::from_slice(config.device_key.as_bytes()),
        MqttBinary::from_slice(b"offline"),
    ) else {
        error!("[MQTT] Invalid MQTT credentials; check CROP_ID and DEVICE_KEY");
        return SessionOutcome::Refused;
    };

    let mut client_id: String<48> = String::new();
    let _ = write!(client_id, "smartpot-{}", config.crop_id);
    let Ok(client_id) = MqttString::from_str(client_id.as_str()) else {
        error!("[MQTT] Invalid MQTT client id; check CROP_ID");
        return SessionOutcome::Refused;
    };

    let mut buffer_provider = AllocBuffer;
    let mut client: Client<'_, &mut T, _, 4, 4, 4, 4> = Client::new(&mut buffer_provider);

    // Última voluntad (LWT) retenida: smartpot/v1/{cropId}/status = offline
    let will = WillOptions::new(status_topic.clone(), offline)
        .retain()
        .at_least_once();

    let connect_options = ConnectOptions::new()
        .clean_start()
        .keep_alive(KeepAlive::Seconds(NonZero::<u16>::MIN.saturating_add(59)))
        .maximum_packet_size(NonZero::<u32>::MIN.saturating_add(MAX_INCOMING_PACKET - 1))
        .user_name(user_name)
        .password(password)
        .will(will);

    if let Err(e) = client
        .connect(transport, &connect_options, Some(client_id))
        .await
    {
        error!("[MQTT] CONNACK failed: {:?}", e);
        // Solo un CONNACK de rechazo significa clave inválida; un corte de red
        // durante el CONNECT es otra cosa y no debe culpar a la clave.
        match e {
            MqttError::Disconnect {
                reason: ReasonCode::BadUserNameOrPassword | ReasonCode::NotAuthorized,
                ..
            } => {
                marker!("No connection to broker: broker rejected the device key");
                marker!("SMARTPOT-MQTT-REFUSED");
            }
            other => marker!("No connection to broker: {:?}", other),
        }
        return SessionOutcome::Refused;
    }
    info!("[MQTT] MQTT session authenticated successfully");

    // Publicar estado online retenido
    let pub_online = PublicationOptions::new(TopicReference::Name(status_topic.clone()))
        .retain()
        .at_least_once();
    if let Err(e) = client
        .publish(&pub_online, rust_mqtt::Bytes::Borrowed(b"online"))
        .await
    {
        error!("[MQTT] Could not publish online status: {:?}", e);
    } else {
        info!(
            "[MQTT] Published 'online' status to {}",
            topics.status.as_str()
        );
    }

    // Publicar capacidades retenidas antes de suscribirse a comandos. Un fallo
    // aquí no impide operar: se registra y se continúa con la suscripción.
    let pub_capabilities = PublicationOptions::new(TopicReference::Name(capabilities_topic))
        .retain()
        .at_least_once();
    match client
        .publish(
            &pub_capabilities,
            rust_mqtt::Bytes::Borrowed(CAPABILITIES_JSON.as_bytes()),
        )
        .await
    {
        Ok(_) => info!(
            "[MQTT] Published retained capabilities to {}",
            topics.capabilities.as_str()
        ),
        Err(e) => warn!("[MQTT] Could not publish capabilities: {:?}", e),
    }

    if let Err(e) = client
        .subscribe(
            commands_topic.as_borrowed().into(),
            SubscriptionOptions::new().at_least_once(),
        )
        .await
    {
        error!("[MQTT] Error subscribing to commands: {:?}", e);
        let _ = with_timeout(CLOSE_TIMEOUT, client.abort()).await;
        return SessionOutcome::Refused;
    }
    info!("[MQTT] Active subscription to {}", topics.commands.as_str());

    // Las instantáneas acumuladas sin conexión se descartan: llevarían la hora
    // de publicación en vez de la de medición y llegarían todas a la vez.
    let mut stale = 0u32;
    while telemetry_channel.try_next_message_pure().is_some() {
        stale += 1;
    }
    if stale > 0 {
        warn!("[MQTT] Discarded {} stale telemetry snapshots", stale);
    }

    BROKER_ONLINE.sender().send(true);
    marker!("MQTT connected as {}", config.crop_id);
    marker!("SMARTPOT-MQTT-UP crop={}", config.crop_id);

    let mut telemetry_seq: u16 = 0;
    let mut ping_ticker = Ticker::every(PING_INTERVAL);
    let mut ping_sent_at: Option<Instant> = None;

    loop {
        beat(Beat::Mqtt);
        // `poll_header` es seguro ante cancelación: si gana otra rama del
        // `select`, no se pierde ningún byte del paquete entrante. El cuerpo se
        // lee fuera del `select` porque `poll_body` no lo es.
        match select4(
            client.poll_header(),
            ack_channel.next_message_pure(),
            telemetry_channel.next_message_pure(),
            ping_ticker.next(),
        )
        .await
        {
            Either4::First(Err(e)) => {
                error!("[MQTT] MQTT poll error: {:?}", e);
                marker!("Connection lost: MQTT poll error");
                break;
            }
            Either4::First(Ok(header)) => match client.poll_body(header).await {
                Ok(Event::Publish(pub_event)) => {
                    let Ok(msg_str) = str::from_utf8(pub_event.message.as_bytes()) else {
                        warn!("[MQTT SUB] Command payload is not UTF-8; discarded");
                        continue;
                    };
                    info!("[MQTT SUB] Message received: {}", msg_str);
                    match parse_command(msg_str) {
                        Ok(cmd) => commands_channel.publish_immediate(cmd),
                        Err(reason) => {
                            warn!("[MQTT SUB] Invalid command discarded: {}", msg_str);
                            marker!("Unreadable command discarded: {}", reason.message());
                            // Si es un JSON con id, emitir ACK de fallo con el motivo
                            if let Ok(id) =
                                crate::protocol::command::extract_str_field(msg_str, "id")
                            {
                                let fail_ack = AckMessage::failure(id.as_str(), reason.message());
                                marker!(
                                    "SMARTPOT-ACK {} FAILED {}",
                                    fail_ack.id.as_str(),
                                    fail_ack.message.as_str()
                                );
                                let ack_json: String<160> = fail_ack.to_json();
                                let pub_opts = PublicationOptions::new(TopicReference::Name(
                                    ack_topic.clone(),
                                ))
                                .at_least_once();
                                if let Err(e) = client
                                    .publish(
                                        &pub_opts,
                                        rust_mqtt::Bytes::Borrowed(ack_json.as_bytes()),
                                    )
                                    .await
                                {
                                    error!("[MQTT] Error publishing failure ACK: {:?}", e);
                                    marker!("Connection lost: Error publishing failure ACK");
                                    break;
                                }
                            }
                        }
                    }
                }
                Ok(Event::Pingresp) => ping_sent_at = None,
                Ok(_) => {}
                Err(e) => {
                    error!("[MQTT] MQTT poll error: {:?}", e);
                    marker!("Connection lost: MQTT poll error");
                    break;
                }
            },

            // Confirmaciones ACK producidas por el ejecutor de actuadores
            Either4::Second(ack) => {
                let ack_json: String<160> = ack.to_json();
                let pub_opts = PublicationOptions::new(TopicReference::Name(ack_topic.clone()))
                    .at_least_once();
                info!(
                    "[MQTT ACK] Publishing to {}: {}",
                    topics.ack.as_str(),
                    ack_json.as_str()
                );
                if let Err(e) = client
                    .publish(&pub_opts, rust_mqtt::Bytes::Borrowed(ack_json.as_bytes()))
                    .await
                {
                    error!("[MQTT] Error publishing ACK confirmation: {:?}", e);
                    marker!("Connection lost: Error publishing ACK confirmation");
                    break;
                }
            }

            // Mensajes de telemetría para publicar
            Either4::Third(telem_msg) => {
                let state = telemetry_state(telem_msg);
                let mut payload: String<255> = String::new();
                let mut packet = [0u8; crate::codec::PACKET_SIZE];
                let body: &[u8] = if config.compact_telemetry {
                    let epoch = clock::now().unwrap_or(0) as u32;
                    let mask =
                        crate::codec::encode_state(&state, telemetry_seq, epoch, &mut packet);
                    telemetry_seq = telemetry_seq.wrapping_add(1);
                    if mask == 0 { &[] } else { &packet }
                } else {
                    state.to_json(&mut payload);
                    if payload.as_str() == "{}" {
                        &[]
                    } else {
                        payload.as_bytes()
                    }
                };

                if !body.is_empty() {
                    let pub_opts =
                        PublicationOptions::new(TopicReference::Name(telemetry_topic.clone()));
                    info!(
                        "[MQTT PUB] Telemetry on {} ({} bytes)",
                        topics.telemetry.as_str(),
                        body.len()
                    );
                    if let Err(e) = client
                        .publish(&pub_opts, rust_mqtt::Bytes::Borrowed(body))
                        .await
                    {
                        error!("[MQTT] Error publishing telemetry: {:?}", e);
                        marker!("Connection lost: Error publishing telemetry");
                        break;
                    }
                }
            }

            // Vida del enlace: un PINGRESP que no llega significa conexión muerta.
            Either4::Fourth(()) => {
                if let Some(sent_at) = ping_sent_at {
                    if Instant::now() - sent_at >= PING_DEADLINE {
                        error!("[MQTT] No PINGRESP from the broker; closing the session");
                        marker!(
                            "Connection lost: No PINGRESP from the broker; closing the session"
                        );
                        break;
                    }
                    continue;
                }
                if let Err(e) = client.ping().await {
                    error!("[MQTT] Could not send PINGREQ: {:?}", e);
                    marker!("Connection lost: Could not send PINGREQ");
                    break;
                }
                ping_sent_at = Some(Instant::now());
            }
        }
    }

    BROKER_ONLINE.sender().send(false);
    marker!("SMARTPOT-MQTT-DOWN");

    // Despedida acotada: en un enlace muerto no se espera al tiempo límite TCP.
    let pub_offline = PublicationOptions::new(TopicReference::Name(status_topic))
        .retain()
        .at_least_once();
    let _ = with_timeout(
        CLOSE_TIMEOUT,
        client.publish(&pub_offline, rust_mqtt::Bytes::Borrowed(b"offline")),
    )
    .await;
    let _ = with_timeout(CLOSE_TIMEOUT, client.abort()).await;
    SessionOutcome::Ended
}
