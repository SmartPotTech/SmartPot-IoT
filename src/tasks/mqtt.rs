extern crate alloc; // Necesario para alloc::format

use core::net::SocketAddrV4;
use core::num::NonZero;
use defmt::{error, info, warn};

use embassy_executor::task;
use embassy_futures::select::{Either, select};
use embassy_net::Stack;
use embassy_net::tcp::TcpSocket as NetTcpSocket;
use embassy_sync::pubsub::{DynPublisher, DynSubscriber};
use embassy_time::{Duration, Timer};

use crate::events::Command;
use embedded_tls::Aes128GcmSha256;
use embedded_tls::{TlsConfig, TlsConnection, TlsContext, UnsecureProvider};
use heapless::String;
use rust_mqtt::buffer::AllocBuffer;
use rust_mqtt::client::event::Event;
use rust_mqtt::client::options::TopicReference;
use rust_mqtt::client::{
    Client,
    options::{ConnectOptions, PublicationOptions, SubscriptionOptions},
};
use rust_mqtt::config::KeepAlive;
use rust_mqtt::types::{MqttBinary, MqttString, TopicName};

use crate::events::telemetry::TelemetryMessage;

// BROKER
const BROKER_IP: &str = "35.172.255.228"; // 35.172.255.228 broker.emqx.io TODO: impl dns if using domain name
const BROKER_PORT: u16 = 8883;

// MQTT TOPICS
const TOPIC_PREFIX: &str = "smartpot/v1/";

// TODO: separate task responsibilities
// TODO: Topic array with FnOne to handle

#[task]
pub async fn mqtt_task(
    stack: Stack<'static>,
    rng: esp_hal::rng::Trng,
    mut telemetry_channel: DynSubscriber<'static, TelemetryMessage>,
    commands_channel: DynPublisher<'static, Command>,
) {
    stack.wait_config_up().await;

    let mut rx_buffer = [0u8; 4096];
    let mut tx_buffer = [0u8; 4096];

    let mut tls_rx = [0u8; 16640];
    let mut tls_tx = [0u8; 16640];

    loop {
        if let Some(config) = stack.config_v4() {
            info!(
                "[MQTT] My IP: {}, MAC: {}, Gateway: {:?}",
                config.address,
                stack.hardware_address(),
                config.gateway
            );
        }

        info!("[MQTT] Trying to connect to {}:{}", BROKER_IP, BROKER_PORT);

        let mut socket = NetTcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        let broker_addr: SocketAddrV4 = alloc::format!("{}:{}", BROKER_IP, BROKER_PORT)
            .parse()
            .expect("Invalid Socket");

        if let Err(e) = socket.connect(broker_addr).await {
            error!("[MQTT] TCP connect failed: {:?}", e);
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }
        info!("[MQTT] TCP Connected");

        // TLS
        let tls_config: TlsConfig = TlsConfig::new()
            .with_server_name(BROKER_IP)
            .enable_rsa_signatures();
        let tls_context = TlsContext::new(
            &tls_config,
            UnsecureProvider::new::<Aes128GcmSha256>(rng.clone()),
        );
        let mut tls = TlsConnection::new(socket, &mut tls_rx, &mut tls_tx);
        if let Err(e) = tls.open(tls_context).await {
            error!("[MQTT] TLS Error: {:?}", e);
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }
        info!("[MQTT] TLS Connected");

        //  MQTT Client
        let mut buffer_provider = AllocBuffer;
        let mut client: Client<'_, &mut TlsConnection<NetTcpSocket, _>, _, 4, 4, 4, 4> =
            Client::new(&mut buffer_provider);
        let connect_options = ConnectOptions::new()
            .clean_start() // ← sin argumento (true por defecto)
            .keep_alive(KeepAlive::Seconds(NonZero::new(60).unwrap()))
            .user_name(MqttString::from_str("esp32-smartpot").unwrap())
            .password(MqttBinary::from_slice(b"password").unwrap());

        if let Err(e) = client
            .connect(
                &mut tls,
                &connect_options,
                Some(MqttString::from_str("esp32-smartpot-v1").unwrap()), // client id
            )
            .await
        {
            error!("[MQTT] MQTT connection failed: {:?}", e);
            Timer::after(Duration::from_secs(5)).await;
            continue;
        }

        info!("[MQTT] MQTT connected successfully!");

        //  create topics
        let topic_base = alloc::format!(
            "{}{}/",
            TOPIC_PREFIX,
            "672ebd43e9227768a15b11df" // stack.hardware_address()
        );
        let bind = alloc::format!("{}commands", topic_base);
        let topic_commands = TopicName::new(MqttString::from_str(&bind).unwrap()).unwrap();

        let topic_sensors_base = alloc::format!("{}sensors", topic_base);

        // Subscribe to commands
        if let Err(e) = client
            .subscribe(
                topic_commands.as_borrowed().into(), // TopicFilter
                SubscriptionOptions::new().at_least_once(),
            )
            .await
        {
            error!("[MQTT] Subscription failed: {:?}", e);
        } else {
            info!("[MQTT] Successfully subscribe to {}", topic_commands);
        }

        loop {
            // Poll (keep connection alive and receive messages from topics)
            match select(client.poll(), Timer::after(Duration::from_secs(20))).await {
                Either::First(poll) => {
                    if let Ok(event) = poll {
                        match &event {
                            Event::Publish(publication) => {
                                let topic = &publication.topic;
                                let message = publication.message.as_bytes();
                                let msg_str = str::from_utf8(message).unwrap();
                                info!("[MQTT SUB] from {}: {}", topic, &msg_str);

                                if let Some((cmd, arg)) = msg_str.split_once(':') {
                                    let command = Command::new(cmd, arg);
                                    match command {
                                        Ok(c) => commands_channel.publish_immediate(c),
                                        Err(_) => warn!("[MQTT SUB] Unknown command: {}", msg_str),
                                    }
                                } else {
                                    warn!("[MQTT SUB] Invalid format: {}", msg_str);
                                }
                            }
                            _ => {
                                info!("[MQTT] received event {}", event);
                            }
                        }
                    } else {
                        error!("[MQTT] Poll error: {:?}", poll);
                        break;
                    }
                }
                Either::Second(_) => {
                    // TODO: wrap in a feature
                    while let Some(message_type) = telemetry_channel.try_next_message_pure() {
                        let mut payload: String<255> = String::new();
                        match message_type {
                            TelemetryMessage::SendOne(m) => {
                                let sensor_topic_str =
                                    alloc::format!("{}/{}", topic_sensors_base, m.get_topic());

                                let topic_sensor = TopicName::new(
                                    MqttString::from_str(&sensor_topic_str).unwrap(),
                                )
                                .unwrap();

                                m.to_json(&mut payload);

                                let topic_reference = TopicReference::Name(topic_sensor);
                                let pub_options = PublicationOptions::new(topic_reference);

                                if let Err(e) = client
                                    .publish(
                                        &pub_options,
                                        rust_mqtt::Bytes::Borrowed(payload.as_bytes()),
                                    )
                                    .await
                                {
                                    error!("[MQTT] Publish failed: {:?}", e);
                                    break;
                                }
                                info!(
                                    "[MQTT] Published at {}: {}",
                                    sensor_topic_str.as_str(),
                                    payload.as_str()
                                );
                            }

                            TelemetryMessage::SendAll(sensor_state) => {
                                sensor_state.to_json(&mut payload);

                                let topic_sensor = TopicName::new(
                                    MqttString::from_str(&topic_sensors_base).unwrap(),
                                )
                                .unwrap();

                                let topic_reference = TopicReference::Name(topic_sensor);
                                let pub_options = PublicationOptions::new(topic_reference);

                                if let Err(e) = client
                                    .publish(
                                        &pub_options,
                                        rust_mqtt::Bytes::Borrowed(payload.as_bytes()),
                                    )
                                    .await
                                {
                                    error!("[MQTT] Publish failed: {:?}", e);
                                    break;
                                }

                                info!(
                                    "[MQTT] Published at {}: {}",
                                    topic_sensors_base.as_str(),
                                    payload.as_str()
                                );
                            }
                        }
                    }

                    /*
                    let mut measures: Vec<Measurements, SENSOR_CH_CAP> = Vec::new();
                    for _ in 0..SENSOR_CH_CAP {
                        match sensor_channel.try_next_message_pure() {
                            Some(m) => measures.push(m).unwrap(),
                            None => { break; }
                        }
                    }

                    for m in &measures {

                        let sensor_topic_str = alloc::format!("{}/{}", topic_sensors_base, m.get_topic());
                        let topic_sensor = TopicName::new(
                            MqttString::from_str(&sensor_topic_str).unwrap()
                        ).unwrap();


                        let mut payload: String<255> = String::new();
                        m.to_json(&mut payload);

                        let topic_reference = TopicReference::Name(topic_sensor);
                        let pub_options = PublicationOptions::new(topic_reference);

                        if let Err(e) = client.publish(
                            &pub_options,
                            rust_mqtt::Bytes::Borrowed(payload.as_bytes()),
                        ).await {
                            error!("[MQTT] Publish failed: {:?}", e);
                            break;
                        } else {
                            info!("[MQTT] Published at {}: {}", sensor_topic_str.as_str(), payload.as_str());
                        }
                    }

                     */
                }
            }
        }

        client.abort().await;
        warn!("[MQTT] Connection lost. Trying to reconnect after 5s...");
    }
}
