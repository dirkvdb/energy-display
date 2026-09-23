use core::{fmt::Write as _, num::NonZero};

use dashboard_core::{
    model::{HourlyData, Update},
    routing::{LIVE_SUBSCRIPTIONS, SUMMARY_TOPIC, decode_update},
};
use embassy_futures::select::{Either, Either4, select, select4};
use embassy_net::{Stack, dns::DnsSocket, tcp::TcpSocket};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::{Channel, TrySendError},
};
use embassy_time::{Duration, Instant, Ticker, Timer, with_timeout};
use embedded_nal_async::{AddrType, Dns};
use jiff::tz::TimeZone;
use rust_mqtt::{
    Bytes,
    buffer::BumpBuffer,
    client::{
        Client, MqttError,
        event::Event,
        options::{
            ConnectOptions, PublicationOptions, SubscriptionOptions, TopicReference,
            UnsubscriptionOptions,
        },
    },
    config::{KeepAlive, SessionExpiryInterval},
    session::Session,
    types::{MqttBinary, MqttString, PacketIdentifier, TopicFilter, TopicName},
};
use serde::Serialize;

use crate::config;

macro_rules! mqtt_log {
    ($($arg:tt)*) => {
        log::info!($($arg)*)
    };
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const SOCKET_TIMEOUT: Duration = Duration::from_secs(120);
const RECONNECT_DELAY: Duration = Duration::from_secs(10);
const PING_INTERVAL: Duration = Duration::from_secs(90);
const SUMMARY_RESTORE_TIMEOUT: Duration = Duration::from_secs(10);

const MAX_SUBSCRIBES: usize = 9;
const RECEIVE_MAXIMUM: usize = 8;
const SEND_MAXIMUM: usize = 1;
const MAX_SUBSCRIPTION_IDENTIFIERS: usize = 0;
const MAX_USER_PROPERTIES: usize = 0;
const MAX_INCOMING_TOPIC_ALIASES: usize = 0;
const MAX_OUTGOING_TOPIC_ALIASES: usize = 0;

pub const TCP_BUFFER_BYTES: usize = 4096;
pub const RECEIVE_SCRATCH_BYTES: usize = 4096;
pub const UPDATE_QUEUE_DEPTH: usize = 8;

pub static UPDATES: Channel<CriticalSectionRawMutex, Update, UPDATE_QUEUE_DEPTH> = Channel::new();

const OVENPLAAT_DIMMER_TOPIC: &str = "home/zigbee/OvenplaatDimmer/set";
const OVENPLAAT_DIMMER_TOGGLE_PAYLOAD: &[u8] = br#"{"state":"TOGGLE"}"#;
const OVENPLAAT_DIMMER_COMMAND_QUEUE_DEPTH: usize = 8;

static OVENPLAAT_DIMMER_COMMANDS: Channel<
    CriticalSectionRawMutex,
    (),
    OVENPLAAT_DIMMER_COMMAND_QUEUE_DEPTH,
> = Channel::new();

pub fn queue_ovenplaat_dimmer_toggle() {
    let _ = OVENPLAAT_DIMMER_COMMANDS.try_send(());
}

const SUMMARY_PAYLOAD_BYTES: usize = RECEIVE_SCRATCH_BYTES;
const SUMMARY_PUBLICATION_QUEUE_DEPTH: usize = 1;

#[derive(Clone, Copy)]
pub struct SummarySnapshot {
    pub timestamp: jiff::Timestamp,
    pub grid_import: HourlyData,
    pub grid_export: HourlyData,
    pub solar_production: HourlyData,
}

#[derive(Clone, Copy)]
pub struct SummaryUpdate {
    pub snapshot: SummarySnapshot,
    /// Time at which a current-day summary was received. This advances the
    /// publication watermark without immediately echoing the retained payload.
    pub summary_received_at: Option<jiff::Timestamp>,
}

pub static SUMMARY_PUBLICATIONS: Channel<
    CriticalSectionRawMutex,
    SummaryUpdate,
    SUMMARY_PUBLICATION_QUEUE_DEPTH,
> = Channel::new();

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
    'static,
    'a,
    TcpSocket<'a>,
    BumpBuffer<'a>,
    MAX_SUBSCRIBES,
    RECEIVE_MAXIMUM,
    SEND_MAXIMUM,
    MAX_SUBSCRIPTION_IDENTIFIERS,
    MAX_USER_PROPERTIES,
    MAX_INCOMING_TOPIC_ALIASES,
    MAX_OUTGOING_TOPIC_ALIASES,
>;
type MqttSession = Session<
    MAX_SUBSCRIBES,
    RECEIVE_MAXIMUM,
    SEND_MAXIMUM,
    MAX_INCOMING_TOPIC_ALIASES,
    MAX_OUTGOING_TOPIC_ALIASES,
>;

pub async fn run(stack: Stack<'static>, buffers: &'static mut Buffers) {
    let mut session = MqttSession::default();
    let mut last_summary_update = None;

    loop {
        stack.wait_config_up().await;
        let dns = DnsSocket::new(stack);
        let broker_address = match with_timeout(
            CONNECT_TIMEOUT,
            dns.get_host_by_name(config::MQTT_BROKER_HOST, AddrType::IPv4),
        )
        .await
        {
            Ok(Ok(core::net::IpAddr::V4(address))) => address,
            Ok(Ok(_)) => {
                log::warn!(
                    "mqtt: DNS lookup for {} returned IPv6",
                    config::MQTT_BROKER_HOST
                );
                Timer::after(RECONNECT_DELAY).await;
                continue;
            }
            Ok(Err(error)) => {
                log::warn!(
                    "mqtt: DNS lookup for {} failed: {:?}",
                    config::MQTT_BROKER_HOST,
                    error
                );
                Timer::after(RECONNECT_DELAY).await;
                continue;
            }
            Err(_) => {
                log::warn!(
                    "mqtt: DNS lookup for {} timed out",
                    config::MQTT_BROKER_HOST
                );
                Timer::after(RECONNECT_DELAY).await;
                continue;
            }
        };
        mqtt_log!(
            "mqtt: connecting to {} ({}):{}",
            config::MQTT_BROKER_HOST,
            broker_address,
            config::MQTT_PORT
        );
        drop(dns);

        let mut socket = TcpSocket::new(stack, &mut buffers.tcp_rx, &mut buffers.tcp_tx);
        socket.set_timeout(Some(SOCKET_TIMEOUT));
        socket.set_keep_alive(Some(PING_INTERVAL));
        socket.set_nagle_enabled(false);

        match with_timeout(
            CONNECT_TIMEOUT,
            socket.connect((broker_address, config::MQTT_PORT)),
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

        match run_session(&mut client, session_present, &mut last_summary_update).await {
            Ok(never) => match never {},
            Err(error) => log::warn!("mqtt: session failed: {:?}", error),
        }

        let _ = client.abort().await;
        session = client.session().clone();
        mqtt_log!("mqtt: reconnecting in {}s", RECONNECT_DELAY.as_secs());
        Timer::after(RECONNECT_DELAY).await;
    }
}

async fn run_session<'a>(
    client: &mut MqttClient<'a>,
    session_present: bool,
    last_summary_update: &mut Option<jiff::Timestamp>,
) -> Result<core::convert::Infallible, MqttError<'a, MAX_USER_PROPERTIES>> {
    clear_summary_publications();
    if session_present {
        mqtt_log!("mqtt: resuming existing session");
    }

    for topic in LIVE_SUBSCRIPTIONS {
        request_subscription(client, topic).await?;
    }

    mqtt_log!("mqtt: restoring daily summary");
    request_subscription(client, SUMMARY_TOPIC).await?;
    let restore = restore_daily_summary(client).await?;
    request_unsubscribe(client, SUMMARY_TOPIC).await?;

    let mut awaiting_restored_snapshot = restore.await_snapshot;
    if !awaiting_restored_snapshot {
        clear_summary_publications();
    }

    mqtt_log!(
        "mqtt: subscribed to {} live filters; daily summary restore complete (session_present={})",
        LIVE_SUBSCRIPTIONS.len(),
        session_present
    );

    let mut ping = Ticker::every(PING_INTERVAL);
    loop {
        match select4(
            SUMMARY_PUBLICATIONS.receive(),
            OVENPLAAT_DIMMER_COMMANDS.receive(),
            client.poll_header(),
            ping.next(),
        )
        .await
        {
            Either4::First(summary) => {
                if !summary_ready_to_publish(&mut awaiting_restored_snapshot, &summary) {
                    continue;
                }
                if let Err(error) =
                    publish_summary_if_needed(client, summary, last_summary_update).await
                {
                    queue_summary_update(summary);
                    return Err(error);
                }
            }
            Either4::Second(()) => {
                if let Err(error) = publish_ovenplaat_dimmer_toggle(client).await {
                    queue_ovenplaat_dimmer_toggle();
                    return Err(error);
                }
            }
            Either4::Third(header) => {
                let event = client.poll_body(header?).await?;
                let update = decode_event(event);

                // The event and all references into scratch have been dropped.
                unsafe { client.buffer_mut().reset() };

                match update {
                    Some(Update::DailySummary(_)) => {
                        log::debug!("mqtt: ignoring daily summary after one-shot restore");
                    }
                    Some(update) => queue_update(update),
                    None => {}
                }
            }
            Either4::Fourth(_) => client.ping().await?,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct RestoreObservation {
    seen: bool,
    await_snapshot: bool,
}

async fn restore_daily_summary<'a>(
    client: &mut MqttClient<'a>,
) -> Result<RestoreObservation, MqttError<'a, MAX_USER_PROPERTIES>> {
    let deadline = Instant::now().saturating_add(SUMMARY_RESTORE_TIMEOUT);
    loop {
        let header = match select(Timer::at(deadline), client.poll_header()).await {
            Either::First(_) => {
                log::warn!("mqtt: no retained daily summary received within 10s");
                return Ok(RestoreObservation::default());
            }
            Either::Second(header) => header?,
        };
        let event = client.poll_body(header).await?;
        let update = decode_event(event);

        // The event and all references into scratch have been dropped.
        unsafe { client.buffer_mut().reset() };

        let Some(update) = update else {
            continue;
        };
        let observation = queue_incoming_update(update);
        if observation.seen {
            return Ok(observation);
        }
    }
}

async fn request_subscription<'a>(
    client: &mut MqttClient<'a>,
    topic: &'static str,
) -> Result<PacketIdentifier, MqttError<'a, MAX_USER_PROPERTIES>> {
    let filter = TopicFilter::new_unchecked(MqttString::from_str_unchecked(topic));
    let packet_identifier = client
        .subscribe(filter, &SubscriptionOptions::new().exactly_once())
        .await?;
    mqtt_log!(
        "mqtt: requested subscription to {} (packet_id={:?})",
        topic,
        packet_identifier
    );
    Ok(packet_identifier)
}

async fn request_unsubscribe<'a>(
    client: &mut MqttClient<'a>,
    topic: &'static str,
) -> Result<(), MqttError<'a, MAX_USER_PROPERTIES>> {
    let filter = TopicFilter::new_unchecked(MqttString::from_str_unchecked(topic));
    let packet_identifier = client
        .unsubscribe(filter, &UnsubscriptionOptions::new())
        .await?;
    mqtt_log!(
        "mqtt: requested unsubscription from {} (packet_id={:?})",
        topic,
        packet_identifier
    );
    Ok(())
}

fn queue_incoming_update(update: Update) -> RestoreObservation {
    let observation = match update {
        Update::DailySummary(summary) => {
            let current = crate::clock::now().is_none_or(|now| summary.is_for(now));
            clear_summary_publications();
            if current {
                mqtt_log!("mqtt: retained daily summary received");
            } else {
                log::warn!("mqtt: retained daily summary is from another day");
            }
            RestoreObservation {
                seen: true,
                await_snapshot: current && crate::clock::utc_now().is_some(),
            }
        }
        _ => RestoreObservation::default(),
    };
    queue_update(update);
    observation
}

fn queue_update(update: Update) {
    if let Err(TrySendError::Full(update)) = UPDATES.try_send(update) {
        // The display only needs the latest snapshot; replace the oldest queued update.
        let _ = UPDATES.try_receive();
        let _ = UPDATES.try_send(update);
    }
}

fn clear_summary_publications() {
    while SUMMARY_PUBLICATIONS.try_receive().is_ok() {}
}

fn summary_ready_to_publish(awaiting_restore: &mut bool, update: &SummaryUpdate) -> bool {
    if *awaiting_restore && update.summary_received_at.is_none() {
        return false;
    }
    *awaiting_restore = false;
    true
}

pub fn queue_summary_update(mut update: SummaryUpdate) {
    if let Err(TrySendError::Full(returned)) = SUMMARY_PUBLICATIONS.try_send(update) {
        update = returned;
        // Keep the newest state, but do not lose a restore watermark while
        // updates coalesce during bursts or a reconnect.
        if let Ok(previous) = SUMMARY_PUBLICATIONS.try_receive()
            && update.summary_received_at.is_none()
        {
            update.summary_received_at = previous.summary_received_at;
        }
        let _ = SUMMARY_PUBLICATIONS.try_send(update);
    }
}

#[derive(Serialize)]
struct DailySummaryWire<'a> {
    timestamp: &'a str,
    grid_import: &'a HourlyData,
    grid_export: &'a HourlyData,
    solar_production: &'a HourlyData,
}

fn encode_summary<'a>(
    snapshot: &'a SummarySnapshot,
    timestamp: &'a str,
    payload: &mut [u8],
) -> Result<usize, serde_json_core::ser::Error> {
    serde_json_core::to_slice(
        &DailySummaryWire {
            timestamp,
            grid_import: &snapshot.grid_import,
            grid_export: &snapshot.grid_export,
            solar_production: &snapshot.solar_production,
        },
        payload,
    )
}

async fn publish_summary_if_needed<'a>(
    client: &mut MqttClient<'a>,
    update: SummaryUpdate,
    last_summary_update: &mut Option<jiff::Timestamp>,
) -> Result<(), MqttError<'a, MAX_USER_PROPERTIES>> {
    let snapshot = update.snapshot;
    if let Some(received_at) = update.summary_received_at {
        *last_summary_update = Some(received_at);
    }
    if last_summary_update.is_some_and(|previous| same_utc_hour(previous, snapshot.timestamp)) {
        return Ok(());
    }

    let mut timestamp = heapless::String::<32>::new();
    if write!(&mut timestamp, "{}", snapshot.timestamp).is_err() {
        log::warn!("mqtt: failed to format daily summary timestamp");
        return Ok(());
    }

    let mut payload = [0u8; SUMMARY_PAYLOAD_BYTES];
    let length = match encode_summary(&snapshot, timestamp.as_str(), &mut payload) {
        Ok(length) => length,
        Err(error) => {
            log::warn!("mqtt: failed to encode daily summary: {:?}", error);
            return Ok(());
        }
    };

    let topic = TopicName::new_unchecked(MqttString::from_str_unchecked(SUMMARY_TOPIC));
    let options = PublicationOptions::new(TopicReference::Name(topic))
        .at_least_once()
        .retain();
    let pending = client
        .session()
        .outbound_publishes
        .first()
        .map(|publication| publication.0);
    if let Some(packet_identifier) = pending {
        client
            .republish(packet_identifier, &options, Bytes::from(&payload[..length]))
            .await?;
    } else {
        client
            .publish(&options, Bytes::from(&payload[..length]))
            .await?;
    }
    *last_summary_update = Some(snapshot.timestamp);
    mqtt_log!("mqtt: published daily summary at {}", timestamp);
    Ok(())
}

async fn publish_ovenplaat_dimmer_toggle<'a>(
    client: &mut MqttClient<'a>,
) -> Result<(), MqttError<'a, MAX_USER_PROPERTIES>> {
    let topic = TopicName::new_unchecked(MqttString::from_str_unchecked(OVENPLAAT_DIMMER_TOPIC));
    let options = PublicationOptions::new(TopicReference::Name(topic)).at_least_once();
    client
        .publish(&options, Bytes::from(OVENPLAAT_DIMMER_TOGGLE_PAYLOAD))
        .await?;
    mqtt_log!("mqtt: toggled {}", OVENPLAAT_DIMMER_TOPIC);
    Ok(())
}

fn same_utc_hour(left: jiff::Timestamp, right: jiff::Timestamp) -> bool {
    let left = left.to_zoned(TimeZone::UTC);
    let right = right.to_zoned(TimeZone::UTC);
    left.date() == right.date() && left.hour() == right.hour()
}

fn decode_event(
    event: Event<'_, MAX_SUBSCRIPTION_IDENTIFIERS, MAX_USER_PROPERTIES>,
) -> Option<Update> {
    let publication = match event {
        Event::Publish(publication) => publication,
        Event::Suback(ack) => {
            mqtt_log!(
                "mqtt: subscription acknowledged (packet_id={:?}, reason={:?})",
                ack.packet_identifier,
                ack.reason_code
            );
            return None;
        }
        Event::Unsuback(ack) => {
            mqtt_log!(
                "mqtt: unsubscription acknowledged (packet_id={:?}, reason={:?})",
                ack.packet_identifier,
                ack.reason_code
            );
            return None;
        }
        _ => return None,
    };

    let topic = publication.topic.name()?.as_ref().as_str();
    match decode_update(topic, publication.message.as_bytes()) {
        Ok(update) => Some(update),
        Err(dashboard_core::routing::DecodeError::IgnoredTopic) => None,
        Err(error) => {
            log::warn!("mqtt: rejected payload on {}: {:?}", topic, error);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(timestamp: jiff::Timestamp) -> SummarySnapshot {
        let mut grid_import = HourlyData::default();
        grid_import.values_at_hour_start[0] = 0.000977;
        grid_import.values[0] = 51.757;
        let mut grid_export = HourlyData::default();
        grid_export.values_at_hour_start[12] = 1.929688;
        grid_export.values[12] = 4227.539;
        let mut solar_production = HourlyData::default();
        solar_production.values_at_hour_start[17] = 46.5;
        solar_production.values[17] = 700.0;
        SummarySnapshot {
            timestamp,
            grid_import,
            grid_export,
            solar_production,
        }
    }

    #[test]
    fn summary_payload_round_trips_through_shared_decoder() {
        let timestamp: jiff::Timestamp = "2026-09-15T17:00:00.236376961Z".parse().unwrap();
        let snapshot = snapshot(timestamp);
        let mut formatted = heapless::String::<32>::new();
        write!(&mut formatted, "{timestamp}").unwrap();
        let mut payload = [0; SUMMARY_PAYLOAD_BYTES];
        let length = encode_summary(&snapshot, formatted.as_str(), &mut payload).unwrap();

        let Update::DailySummary(summary) =
            decode_update(SUMMARY_TOPIC, &payload[..length]).unwrap()
        else {
            panic!("encoded summary did not decode as a daily summary");
        };
        assert_eq!(summary.date, jiff::civil::date(2026, 9, 15));
        assert_eq!(summary.grid_import.values[0], 51.757);
        assert_eq!(summary.grid_export.values[12], 4227.539);
        assert_eq!(summary.solar_production.values[17], 700.0);
    }

    #[test]
    fn partial_snapshots_are_suppressed_until_restored_state_arrives() {
        let timestamp: jiff::Timestamp = "2026-09-15T17:00:00Z".parse().unwrap();
        let mut awaiting_restore = true;
        let partial = SummaryUpdate {
            snapshot: snapshot(timestamp),
            summary_received_at: None,
        };
        let restored = SummaryUpdate {
            snapshot: snapshot(timestamp),
            summary_received_at: Some(timestamp),
        };

        assert!(!summary_ready_to_publish(&mut awaiting_restore, &partial));
        assert!(awaiting_restore);
        assert!(summary_ready_to_publish(&mut awaiting_restore, &restored));
        assert!(!awaiting_restore);
    }

    #[test]
    fn hourly_publish_guard_uses_utc_date_and_hour() {
        let first: jiff::Timestamp = "2026-09-15T17:00:00Z".parse().unwrap();
        let same: jiff::Timestamp = "2026-09-15T17:59:59Z".parse().unwrap();
        let next: jiff::Timestamp = "2026-09-15T18:00:00Z".parse().unwrap();
        let next_day: jiff::Timestamp = "2026-09-16T17:00:00Z".parse().unwrap();

        assert!(same_utc_hour(first, same));
        assert!(!same_utc_hour(first, next));
        assert!(!same_utc_hour(first, next_day));
    }
}
