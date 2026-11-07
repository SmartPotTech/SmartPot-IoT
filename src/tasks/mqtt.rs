//! Cliente MQTT robusto con soporte de contrato v1 (telemetría, comandos, confirmación ACK y LWT).
//!
//! La conexión usa TLS 1.3 con verificación de la CA propia del broker cuando
//! `USE_TLS` está activo (valor por defecto). El epoch Unix se obtiene por SNTP
//! antes del handshake y nunca se cae a una conexión sin verificar: si falta la
//! CA o el reloj, el cliente registra el error y reintenta.

extern crate alloc;

use core::net::{Ipv4Addr, SocketAddrV4};
use core::num::NonZero;
use core::str;

use defmt::{error, info, warn};
use embassy_executor::task;
use embassy_futures::select::{Either3, select3};
use embassy_net::dns::DnsQueryType;
use embassy_net::tcp::TcpSocket as NetTcpSocket;
use embassy_net::{IpAddress, Stack};
use embassy_sync::pubsub::{DynPublisher, DynSubscriber};
use embassy_time::{Duration, Timer};

use embedded_tls::{TlsConfig, TlsConnection, TlsContext};
use heapless::String;
use rust_mqtt::buffer::AllocBuffer;
use rust_mqtt::client::Client;
use rust_mqtt::client::event::Event;
use rust_mqtt::client::options::{
    ConnectOptions, PublicationOptions, SubscriptionOptions, TopicReference, WillOptions,
};
use rust_mqtt::config::KeepAlive;
use rust_mqtt::types::{MqttBinary, MqttString, TopicName};

use crate::capabilities::CAPABILITIES_JSON;
use crate::clock;
use crate::config::{self, Config};
use crate::events::telemetry::TelemetryMessage;
use crate::events::{AckMessage, BROKER_ONLINE, Command};
use crate::protocol::{TopicResolver, parse_command};
use crate::tls::VerifyingProvider;

/// Tamaño del búfer para la cadena de certificados del servidor durante el handshake.
const TLS_CERT_BUFFER_SIZE: usize = 2048;

#[task]
pub async fn mqtt_task(
    stack: Stack<'static>,
    rng: esp_hal::rng::Trng,
    config: Config,
    mut telemetry_channel: DynSubscriber<'static, TelemetryMessage>,
    commands_channel: DynPublisher<'static, Command>,
    mut ack_channel: DynSubscriber<'static, AckMessage>,
) {
    stack.wait_config_up().await;

    let mut rx_buffer = [0u8; 4096];
    let mut tx_buffer = [0u8; 4096];

    let mut tls_rx = [0u8; 16640];
    let mut tls_tx = [0u8; 16640];

    let resolver = TopicResolver::new(config.crop_id);
    let topic_telemetry_str: String<64> = resolver.telemetry();
    let topic_commands_str: String<64> = resolver.commands();
    let topic_ack_str: String<64> = resolver.ack();
    let topic_status_str: String<64> = resolver.status();
    let topic_capabilities_str: String<64> = resolver.capabilities();

    loop {
        if let Some(net_cfg) = stack.config_v4() {
            info!(
                "[MQTT] Local IP: {}, Gateway: {:?}",
                net_cfg.address, net_cfg.gateway
            );
        }

        // Precondiciones de TLS: hora válida y CA propia. Se comprueban antes
        // de abrir el socket para no mantener conexiones TCP sin verificar.
        let ca_der = if config.use_tls {
            while !clock::is_synced() {
                warn!("[MQTT] Waiting for SNTP time synchronization before TLS");
                Timer::after(Duration::from_secs(2)).await;
            }

            match config::ca_cert_der::<TLS_CERT_BUFFER_SIZE>(config.ca_cert_pem) {
                Ok(der) => der,
                Err(_) => {
                    error!(
                        "[MQTT] CA_CERT_PEM is missing or invalid; refusing to connect without certificate verification"
                    );
                    Timer::after(Duration::from_secs(5)).await;
                    continue;
                }
            }
        } else {
            heapless::Vec::new()
        };

        info!(
            "[MQTT] Connecting to {}:{} (Crop: {})",
            config.broker_host, config.broker_port, config.crop_id
        );

        let broker_ip = match resolve_broker(stack, config.broker_host).await {
            Some(ip) => ip,
            None => {
                Timer::after(Duration::from_secs(5)).await;
                continue;
            }
        };
        let broker_addr = SocketAddrV4::new(broker_ip, config.broker_port);

        let mut socket = NetTcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        if let Err(e) = socket.connect(broker_addr).await {
            error!("[MQTT] TCP connection failed: {:?}", e);
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }
        info!("[MQTT] TCP connection established");

        if config.use_tls {
            let tls_config = TlsConfig::new()
                .with_server_name(config.broker_host)
                .enable_rsa_signatures();
            let provider = VerifyingProvider::new(rng.clone(), ca_der.as_slice());
            let tls_context = TlsContext::new(&tls_config, provider);
            let mut tls = TlsConnection::new(socket, &mut tls_rx, &mut tls_tx);
            if let Err(e) = tls.open(tls_context).await {
                error!("[MQTT] TLS handshake failed: {:?}", e);
                Timer::after(Duration::from_secs(5)).await;
                continue;
            }
            info!("[MQTT] TLS session established; broker certificate verified");

            run_mqtt_client(
                &mut tls,
                &config,
                &topic_telemetry_str,
                &topic_commands_str,
                &topic_ack_str,
                &topic_status_str,
                &topic_capabilities_str,
                &mut telemetry_channel,
                &commands_channel,
                &mut ack_channel,
            )
            .await;
        } else {
            warn!("[MQTT] TLS is disabled; connecting without certificate verification");

            run_mqtt_client(
                &mut socket,
                &config,
                &topic_telemetry_str,
                &topic_commands_str,
                &topic_ack_str,
                &topic_status_str,
                &topic_capabilities_str,
                &mut telemetry_channel,
                &commands_channel,
                &mut ack_channel,
            )
            .await;
        }

        warn!("[MQTT] Connection closed. Retrying in 5 s...");
        Timer::after(Duration::from_secs(5)).await;
    }
}

/// Resuelve el host del broker a IPv4 mediante el DNS del stack de red.
///
/// `dns_query` acepta tanto nombres como literales IP, por lo que
/// `BROKER_HOST=127.0.0.1` sigue funcionando sin configuración adicional.
#[allow(unreachable_patterns)]
async fn resolve_broker(stack: Stack<'static>, host: &str) -> Option<Ipv4Addr> {
    match stack.dns_query(host, DnsQueryType::A).await {
        Ok(addresses) => addresses.iter().find_map(|address| match address {
            IpAddress::Ipv4(ipv4) => Some(Ipv4Addr::from(ipv4.octets())),
            // La variante IPv6 solo existe si se compila proto-ipv6.
            _ => None,
        }),
        Err(e) => {
            error!("[MQTT] DNS resolution failed for {}: {:?}", host, e);
            None
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_mqtt_client<T>(
    transport: &mut T,
    config: &Config,
    topic_telemetry_str: &str,
    topic_commands_str: &str,
    topic_ack_str: &str,
    topic_status_str: &str,
    topic_capabilities_str: &str,
    telemetry_channel: &mut DynSubscriber<'static, TelemetryMessage>,
    commands_channel: &DynPublisher<'static, Command>,
    ack_channel: &mut DynSubscriber<'static, AckMessage>,
) where
    T: embedded_io_async::Read + embedded_io_async::Write,
{
    let mut buffer_provider = AllocBuffer;
    let mut client: Client<'_, &mut T, _, 4, 4, 4, 4> = Client::new(&mut buffer_provider);

    // Configurar última voluntad (LWT) retenida: smartpot/v1/{cropId}/status = offline
    let will = WillOptions::new(
        TopicName::new(MqttString::from_str(topic_status_str).unwrap()).unwrap(),
        MqttBinary::from_slice(b"offline").unwrap(),
    )
    .retain()
    .at_least_once();

    let connect_options = ConnectOptions::new()
        .clean_start()
        .keep_alive(KeepAlive::Seconds(NonZero::new(60).unwrap()))
        .user_name(MqttString::from_str(config.crop_id).unwrap())
        .password(MqttBinary::from_slice(config.device_key.as_bytes()).unwrap())
        .will(will);

    let client_id_str = alloc::format!("smartpot-{}", config.crop_id);
    if let Err(e) = client
        .connect(
            transport,
            &connect_options,
            Some(MqttString::from_str(&client_id_str).unwrap()),
        )
        .await
    {
        error!("[MQTT] CONNACK failed: {:?}", e);
        return;
    }
    info!("[MQTT] MQTT session authenticated successfully");

    // Publicar estado online retenido
    let status_topic_name =
        TopicName::new(MqttString::from_str(topic_status_str).unwrap()).unwrap();
    let pub_online = PublicationOptions::new(TopicReference::Name(status_topic_name.clone()))
        .retain()
        .at_least_once();
    if let Err(e) = client
        .publish(&pub_online, rust_mqtt::Bytes::Borrowed(b"online"))
        .await
    {
        error!("[MQTT] Could not publish online status: {:?}", e);
    } else {
        info!("[MQTT] Published 'online' status to {}", topic_status_str);
    }

    // Publicar capacidades retenidas antes de suscribirse a comandos. Un fallo
    // aquí no impide operar: se registra y se continúa con la suscripción.
    let capabilities_topic_name =
        TopicName::new(MqttString::from_str(topic_capabilities_str).unwrap()).unwrap();
    let pub_capabilities = PublicationOptions::new(TopicReference::Name(capabilities_topic_name))
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
            topic_capabilities_str
        ),
        Err(e) => warn!("[MQTT] Could not publish capabilities: {:?}", e),
    }

    // Suscribirse al tópico de comandos
    let commands_topic_name =
        TopicName::new(MqttString::from_str(topic_commands_str).unwrap()).unwrap();
    if let Err(e) = client
        .subscribe(
            commands_topic_name.as_borrowed().into(),
            SubscriptionOptions::new().at_least_once(),
        )
        .await
    {
        error!("[MQTT] Error subscribing to commands: {:?}", e);
        return;
    }
    info!("[MQTT] Active subscription to {}", topic_commands_str);
    BROKER_ONLINE.sender().send(true);

    let ack_topic_name = TopicName::new(MqttString::from_str(topic_ack_str).unwrap()).unwrap();
    let telemetry_topic_name =
        TopicName::new(MqttString::from_str(topic_telemetry_str).unwrap()).unwrap();
    let mut telemetry_seq: u16 = 0;

    loop {
        match select3(
            client.poll(),
            ack_channel.next_message_pure(),
            telemetry_channel.next_message_pure(),
        )
        .await
        {
            // Eventos del cliente MQTT (mensajes entrantes o pingreq)
            Either3::First(poll_res) => match poll_res {
                Ok(Event::Publish(pub_event)) => {
                    let msg_bytes = pub_event.message.as_bytes();
                    if let Ok(msg_str) = str::from_utf8(msg_bytes) {
                        info!("[MQTT SUB] Message received: {}", msg_str);
                        match parse_command(msg_str) {
                            Ok(cmd) => {
                                commands_channel.publish_immediate(cmd);
                            }
                            Err(reason) => {
                                warn!("[MQTT SUB] Invalid command discarded: {}", msg_str);
                                // Si es un JSON con id, emitir ACK de fallo con el motivo
                                if let Ok(id) =
                                    crate::protocol::command::extract_str_field(msg_str, "id")
                                {
                                    let fail_ack =
                                        AckMessage::failure(id.as_str(), reason.message());
                                    let ack_json: String<160> = fail_ack.to_json();
                                    let pub_opts = PublicationOptions::new(TopicReference::Name(
                                        ack_topic_name.clone(),
                                    ))
                                    .at_least_once();
                                    let _ = client
                                        .publish(
                                            &pub_opts,
                                            rust_mqtt::Bytes::Borrowed(ack_json.as_bytes()),
                                        )
                                        .await;
                                }
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    error!("[MQTT] MQTT poll error: {:?}", e);
                    break;
                }
            },

            // Confirmaciones ACK producidas por el ejecutor de actuadores
            Either3::Second(ack) => {
                let ack_json: String<160> = ack.to_json();
                let pub_opts =
                    PublicationOptions::new(TopicReference::Name(ack_topic_name.clone()))
                        .at_least_once();
                info!(
                    "[MQTT ACK] Publishing to {}: {}",
                    topic_ack_str,
                    ack_json.as_str()
                );
                if let Err(e) = client
                    .publish(&pub_opts, rust_mqtt::Bytes::Borrowed(ack_json.as_bytes()))
                    .await
                {
                    error!("[MQTT] Error publishing ACK confirmation: {:?}", e);
                    break;
                }
            }

            // Mensajes de telemetría para publicar
            Either3::Third(telem_msg) => {
                let state = match telem_msg {
                    TelemetryMessage::SendAll(state) => state,
                    TelemetryMessage::SendOne(m) => {
                        let mut state = crate::protocol::SensorState::default();
                        match m {
                            crate::events::Measurements::BME280((t, h, p)) => {
                                state.update_bme280(t, h, p);
                            }
                            crate::events::Measurements::BH1750(l) => {
                                state.update_lux(l);
                            }
                            crate::events::Measurements::PH(p) => {
                                state.update_ph(p);
                            }
                            crate::events::Measurements::TDS(t) => {
                                state.update_tds(t);
                            }
                            crate::events::Measurements::SoilMoisture(sm) => {
                                state.update_soil_moisture(sm);
                            }
                        }
                        state
                    }
                };
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
                        PublicationOptions::new(TopicReference::Name(telemetry_topic_name.clone()));
                    info!(
                        "[MQTT PUB] Telemetry on {} ({} bytes)",
                        topic_telemetry_str,
                        body.len()
                    );
                    if let Err(e) = client
                        .publish(&pub_opts, rust_mqtt::Bytes::Borrowed(body))
                        .await
                    {
                        error!("[MQTT] Error publishing telemetry: {:?}", e);
                        break;
                    }
                }
            }
        }
    }

    BROKER_ONLINE.sender().send(false);

    // Publicar estado offline antes de desconectar
    let pub_offline = PublicationOptions::new(TopicReference::Name(status_topic_name))
        .retain()
        .at_least_once();
    let _ = client
        .publish(&pub_offline, rust_mqtt::Bytes::Borrowed(b"offline"))
        .await;
    let _ = client.abort().await;
}
