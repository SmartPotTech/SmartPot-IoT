//! Cliente MQTT robusto con soporte de contrato v1 (telemetría, comandos, confirmación ACK y LWT).

extern crate alloc;

use core::net::SocketAddrV4;
use core::num::NonZero;
use core::str;
use defmt::{error, info, warn};

use embassy_executor::task;
use embassy_futures::select::{select3, Either3};
use embassy_net::tcp::TcpSocket as NetTcpSocket;
use embassy_net::Stack;
use embassy_sync::pubsub::{DynPublisher, DynSubscriber};
use embassy_time::{Duration, Timer};
use embassy_futures::select::{select, Either};

use embedded_tls::{Aes128GcmSha256, TlsConfig, TlsConnection, TlsContext, UnsecureProvider};
use heapless::String;
use rust_mqtt::buffer::AllocBuffer;
use rust_mqtt::client::event::Event;
use rust_mqtt::client::options::{
    ConnectOptions, PublicationOptions, SubscriptionOptions, TopicReference, WillOptions,
};
use rust_mqtt::client::Client;
use rust_mqtt::config::KeepAlive;
use rust_mqtt::types::{MqttBinary, MqttString, TopicName};

use crate::config::Config;
use crate::events::telemetry::TelemetryMessage;
use crate::events::{AckMessage, Command};
use crate::protocol::{parse_command, TopicResolver};

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

    loop {
        if let Some(net_cfg) = stack.config_v4() {
            info!(
                "[MQTT] IP local: {}, Gateway: {:?}",
                net_cfg.address, net_cfg.gateway
            );
        }

        info!(
            "[MQTT] Conectando a {}:{} (Cultivo: {})",
            config.broker_host, config.broker_port, config.crop_id
        );

        let mut socket = NetTcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        let broker_addr: SocketAddrV4 = match alloc::format!("{}:{}", config.broker_host, config.broker_port).parse() {
            Ok(addr) => addr,
            Err(e) => {
                error!("[MQTT] Dirección de broker inválida: {:?}", defmt::Debug2Format(&e));
                Timer::after(Duration::from_secs(5)).await;
                continue;
            }
        };

        if let Err(e) = socket.connect(broker_addr).await {
            error!("[MQTT] Conexión TCP fallida: {:?}", e);
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }
        info!("[MQTT] Conexión TCP establecida");

        if config.use_tls {
            let tls_config: TlsConfig<> = TlsConfig::new()
                .with_server_name(config.broker_host)
                .enable_rsa_signatures();
            let tls_context = TlsContext::new(
                &tls_config,
                UnsecureProvider::new::<Aes128GcmSha256>(rng.clone()),
            );
            let mut tls = TlsConnection::new(socket, &mut tls_rx, &mut tls_tx);
            if let Err(e) = tls.open(tls_context).await {
                error!("[MQTT] Fallo en enlace TLS: {:?}", e);
                Timer::after(Duration::from_secs(5)).await;
                continue;
            }
            info!("[MQTT] Enlace TLS verificado");

            run_mqtt_client(
                &mut tls,
                &config,
                &topic_telemetry_str,
                &topic_commands_str,
                &topic_ack_str,
                &topic_status_str,
                &mut telemetry_channel,
                &commands_channel,
                &mut ack_channel,
            )
            .await;
        } else {
            run_mqtt_client(
                &mut socket,
                &config,
                &topic_telemetry_str,
                &topic_commands_str,
                &topic_ack_str,
                &topic_status_str,
                &mut telemetry_channel,
                &commands_channel,
                &mut ack_channel,
            )
            .await;
        }
        info!("[MQTT] TLS Connected");

        warn!("[MQTT] Conexión cerrada. Reintentando en 5 s...");
        Timer::after(Duration::from_secs(5)).await;
    }
}

async fn run_mqtt_client<T>(
    transport: &mut T,
    config: &Config,
    topic_telemetry_str: &str,
    topic_commands_str: &str,
    topic_ack_str: &str,
    topic_status_str: &str,
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
        error!("[MQTT] Fallo en CONNACK: {:?}", e);
        return;
    }
    info!("[MQTT] Sesión MQTT autenticada con éxito");

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
        error!("[MQTT] No se pudo publicar status online: {:?}", e);
    } else {
        info!("[MQTT] Publicado status 'online' en {}", topic_status_str);
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
        error!("[MQTT] Error al suscribirse a comandos: {:?}", e);
        return;
    }
    info!("[MQTT] Suscripción activa a {}", topic_commands_str);

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
                        info!("[MQTT SUB] Mensaje recibido: {}", msg_str);
                        match parse_command(msg_str) {
                            Ok(cmd) => {
                                commands_channel.publish_immediate(cmd);
                            }
                            Err(_) => {
                                warn!("[MQTT SUB] Comando inválido descartado: {}", msg_str);
                                // Si es un JSON con id, emitir ACK de fallo
                                if let Ok(id) = crate::protocol::command::extract_str_field(msg_str, "id") {
                                    let fail_ack = AckMessage::failure(
                                        id.as_str(),
                                        "Comando inválido o actuador desconocido",
                                    );
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
                    error!("[MQTT] Error en sondeo MQTT: {:?}", e);
                    break;
                }
            },

            // Confirmaciones ACK producidas por el ejecutor de actuadores
            Either3::Second(ack) => {
                let ack_json: String<160> = ack.to_json();
                let pub_opts =
                    PublicationOptions::new(TopicReference::Name(ack_topic_name.clone()))
                        .at_least_once();
                info!("[MQTT ACK] Publicando en {}: {}", topic_ack_str, ack_json.as_str());
                if let Err(e) = client
                    .publish(&pub_opts, rust_mqtt::Bytes::Borrowed(ack_json.as_bytes()))
                    .await
                {
                    error!("[MQTT] Error al publicar confirmación ACK: {:?}", e);
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
                    let mask = crate::codec::encode_state(&state, telemetry_seq, 0, &mut packet);
                    telemetry_seq = telemetry_seq.wrapping_add(1);
                    if mask == 0 { &[] } else { &packet }
                } else {
                    state.to_json(&mut payload);
                    if payload.as_str() == "{}" { &[] } else { payload.as_bytes() }
                };

                if !body.is_empty() {
                    let pub_opts = PublicationOptions::new(TopicReference::Name(
                        telemetry_topic_name.clone(),
                    ));
                    info!(
                        "[MQTT PUB] Telemetría en {} ({} bytes)",
                        topic_telemetry_str,
                        body.len()
                    );
                    if let Err(e) = client
                        .publish(&pub_opts, rust_mqtt::Bytes::Borrowed(body))
                        .await
                    {
                        error!("[MQTT] Error al publicar telemetría: {:?}", e);
                        break;
                    }
                }
            }
        }
    }

    // Publicar estado offline antes de desconectar
    let pub_offline = PublicationOptions::new(TopicReference::Name(status_topic_name))
        .retain()
        .at_least_once();
    let _ = client
        .publish(&pub_offline, rust_mqtt::Bytes::Borrowed(b"offline"))
        .await;
    let _ = client.abort().await;
}