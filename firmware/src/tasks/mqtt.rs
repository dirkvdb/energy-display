use core::num::NonZero;

use dashboard_core::{
    model::Update,
    routing::{LIVE_SUBSCRIPTIONS, decode_update},
};
use embassy_futures::select::{Either, select};
use embassy_net::{Stack, tcp::TcpSocket};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Duration, Ticker, Timer, with_timeout};
use rust_mqtt::{
    buffer::BumpBuffer,
    client::{
        Client, MqttError,
        event::Event,
        options::{ConnectOptions, SubscriptionOptions},
    },
    config::{KeepAlive, SessionExpiryInterval},
    session::Session,
    types::{MqttBinary, MqttString, ReasonCode, TopicFilter},
};

use crate::config;

macro_rules! mqtt_log {
    ($($arg:tt)*) => {
        log::info!($($arg)*)
    };
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const SOCKET_TIMEOUT: Duration = Duration::from_secs(30);
const RECONNECT_DELAY: Duration = Duration::from_secs(10);
const PING_INTERVAL: Duration = Duration::from_secs(90);

const MAX_SUBSCRIBES: usize = 1;
const RECEIVE_MAXIMUM: usize = 8;
const SEND_MAXIMUM: usize = 1;
const MAX_SUBSCRIPTION_IDENTIFIERS: usize = 0;

pub const TCP_BUFFER_BYTES: usize = 4096;
pub const RECEIVE_SCRATCH_BYTES: usize = 4096;
pub const UPDATE_QUEUE_DEPTH: usize = 8;

pub static UPDATES: Channel<CriticalSectionRawMutex, Update, UPDATE_QUEUE_DEPTH> = Channel::new();

pub struct Buffers {
    tcp_rx: [u8; TCP_BUFFER_BYTES],
    tcp_tx: [u8; TCP_BUFFER_BYTES],
    receive_scratch: [u8; RECEIVE_SCRATCH_BYTES],
}

impl Buffers {
    pub const fn new() -> Self {
        Self {
            tcp_rx: [0; TCP_BUFFER_BYTES],
            tcp_tx: [0; TCP_BUFFER_BYTES],
            receive_scratch: [0; RECEIVE_SCRATCH_BYTES],
        }
    }
}

type MqttClient<'a> = Client<
    'a,
    TcpSocket<'a>,
    BumpBuffer<'a>,
    MAX_SUBSCRIBES,
    RECEIVE_MAXIMUM,
    SEND_MAXIMUM,
    MAX_SUBSCRIPTION_IDENTIFIERS,
>;
type MqttSession = Session<RECEIVE_MAXIMUM, SEND_MAXIMUM>;

pub async fn run(stack: Stack<'static>, buffers: &'static mut Buffers) {
    let mut session = MqttSession::default();

    loop {
        stack.wait_config_up().await;
        mqtt_log!(
            "mqtt: connecting to {}:{}",
            config::MQTT_BROKER_ADDRESS,
            config::MQTT_PORT
        );

        let mut socket = TcpSocket::new(stack, &mut buffers.tcp_rx, &mut buffers.tcp_tx);
        socket.set_timeout(Some(SOCKET_TIMEOUT));
        socket.set_keep_alive(Some(PING_INTERVAL));
        socket.set_nagle_enabled(false);

        match with_timeout(
            CONNECT_TIMEOUT,
            socket.connect((config::MQTT_BROKER_ADDRESS, config::MQTT_PORT)),
        )
        .await
        {
            Ok(Ok(())) => mqtt_log!("mqtt: TCP connected"),
            Ok(Err(error)) => {
                log::warn!("mqtt: TCP connection failed: {:?}", error);
                Timer::after(RECONNECT_DELAY).await;
                continue;
            }
            Err(_) => {
                log::warn!("mqtt: TCP connection timed out");
                Timer::after(RECONNECT_DELAY).await;
                continue;
            }
        }

        let connect_options = ConnectOptions::new()
            .session_expiry_interval(SessionExpiryInterval::NeverEnd)
            .keep_alive(KeepAlive::Seconds(
                NonZero::new(config::MQTT_KEEP_ALIVE_SECONDS).unwrap(),
            ))
            .maximum_packet_size(NonZero::new(RECEIVE_SCRATCH_BYTES as u32).unwrap())
            .user_name(MqttString::from_str_unchecked(config::MQTT_USERNAME))
            .password(MqttBinary::from_slice_unchecked(
                config::MQTT_PASSWORD.as_bytes(),
            ));

        let mut bump = BumpBuffer::new(&mut buffers.receive_scratch);
        let mut client = MqttClient::with_session(session, &mut bump);

        let session_present = match with_timeout(
            CONNECT_TIMEOUT,
            client.connect(
                socket,
                &connect_options,
                Some(MqttString::from_str_unchecked(config::MQTT_CLIENT_ID)),
            ),
        )
        .await
        {
            Ok(Ok(info)) => info.session_present,
            Ok(Err(error)) => {
                log::warn!("mqtt: handshake failed: {:?}", error);
                session = client.session().clone();
                Timer::after(RECONNECT_DELAY).await;
                continue;
            }
            Err(_) => {
                log::warn!("mqtt: handshake timed out");
                session = client.session().clone();
                Timer::after(RECONNECT_DELAY).await;
                continue;
            }
        };

        // The CONNACK event has been dropped, so no references into scratch remain.
        unsafe { client.buffer_mut().reset() };

        match run_session(&mut client, session_present).await {
            Ok(never) => match never {},
            Err(error) => log::warn!("mqtt: session failed: {:?}", error),
        }

        client.abort().await;
        session = client.session().clone();
        mqtt_log!("mqtt: reconnecting in {}s", RECONNECT_DELAY.as_secs());
        Timer::after(RECONNECT_DELAY).await;
    }
}

async fn run_session<'a>(
    client: &mut MqttClient<'a>,
    session_present: bool,
) -> Result<core::convert::Infallible, MqttError<'a>> {
    for topic in LIVE_SUBSCRIPTIONS {
        subscribe(client, topic).await?;
    }

    mqtt_log!(
        "mqtt: subscribed to {} live filters (session_present={})",
        LIVE_SUBSCRIPTIONS.len(),
        session_present
    );

    let mut ping = Ticker::every(PING_INTERVAL);
    loop {
        match select(client.poll_header(), ping.next()).await {
            Either::First(header) => {
                let event = client.poll_body(header?).await?;
                let update = decode_event(event);

                // The event and all references into scratch have been dropped.
                unsafe { client.buffer_mut().reset() };

                if let Some(update) = update {
                    UPDATES.send(update).await;
                }
            }
            Either::Second(_) => client.ping().await?,
        }
    }
}

async fn subscribe<'a>(
    client: &mut MqttClient<'a>,
    topic: &'static str,
) -> Result<(), MqttError<'a>> {
    let filter = TopicFilter::new_unchecked(MqttString::from_str_unchecked(topic));
    let packet_identifier = client
        .subscribe(filter, SubscriptionOptions::new().exactly_once())
        .await?;

    loop {
        let header = client.poll_header().await?;
        let event = client.poll_body(header).await?;
        let mut acknowledged = false;
        let update = match event {
            Event::Suback(ack) if ack.packet_identifier == packet_identifier => {
                if ack.reason_code != ReasonCode::GrantedQoS2 {
                    log::warn!(
                        "mqtt: broker rejected QoS 2 subscription to {}: {:?}",
                        topic,
                        ack.reason_code
                    );
                    return Err(MqttError::Server);
                }
                acknowledged = true;
                None
            }
            event => decode_event(event),
        };

        // The event and all references into scratch have been dropped.
        unsafe { client.buffer_mut().reset() };

        if let Some(update) = update {
            UPDATES.send(update).await;
        }
        if acknowledged {
            mqtt_log!("mqtt: subscribed {}", topic);
            return Ok(());
        }
    }
}

fn decode_event(event: Event<'_, MAX_SUBSCRIPTION_IDENTIFIERS>) -> Option<Update> {
    let Event::Publish(publication) = event else {
        return None;
    };

    let topic = publication.topic.as_ref().as_str();
    match decode_update(topic, publication.message.as_bytes()) {
        Ok(update) => Some(update),
        Err(dashboard_core::routing::DecodeError::IgnoredTopic) => None,
        Err(error) => {
            log::warn!("mqtt: rejected payload on {}: {:?}", topic, error);
            None
        }
    }
}
