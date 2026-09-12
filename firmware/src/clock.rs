use core::{cell::RefCell, fmt, net::SocketAddr};

use dashboard_core::LocalDateTime;
use embassy_net::{
    Stack,
    udp::{BindError, PacketMetadata, UdpSocket},
};
use embassy_sync::blocking_mutex::{Mutex, raw::CriticalSectionRawMutex};
use embassy_time::{Duration, Instant, Timer, with_timeout};
use jiff::{Timestamp, tz::TimeZone};
use sntpc::{NtpContext, NtpResult, fraction_to_nanoseconds, get_time};
use sntpc_net_embassy::UdpSocketWrapper;
use sntpc_time_embassy::EmbassyTimestampGenerator;

use crate::config;
const NTP_PORT: u16 = 123;
const NTP_PACKET_SIZE: usize = 48;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_INTERVAL: Duration = Duration::from_secs(60);
const SYNC_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

static TIME_ZONE: TimeZone = jiff::tz::get!("Europe/Brussels");
static CLOCK: Mutex<CriticalSectionRawMutex, RefCell<Option<Anchor>>> =
    Mutex::new(RefCell::new(None));

macro_rules! clock_log {
    ($level:ident, $($arg:tt)*) => {
        log::$level!($($arg)*)
    };
}

#[derive(Clone, Copy)]
struct Anchor {
    utc: Timestamp,
    instant: Instant,
}

#[derive(Debug)]
enum Error {
    Bind(BindError),
    RequestTimeout,
    Sntp(sntpc::Error),
    TimestampOutOfRange,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bind(error) => write!(formatter, "UDP bind error: {error:?}"),
            Self::RequestTimeout => formatter.write_str("request timed out"),
            Self::Sntp(error) => write!(formatter, "SNTP error: {error:?}"),
            Self::TimestampOutOfRange => {
                formatter.write_str("server timestamp is outside the UTC range 2000..=2099")
            }
        }
    }
}

/// Returns Brussels wall time, or `None` until the first successful synchronization.
pub fn now() -> Option<LocalDateTime> {
    local_datetime(utc_now()?)
}

/// Returns UTC wall time, or `None` until the first successful synchronization.
pub fn utc_now() -> Option<Timestamp> {
    let anchor = CLOCK.lock(|clock| *clock.borrow())?;
    timestamp_at(Some(anchor), Instant::now())
}

/// Synchronizes UTC over SNTP and keeps the last anchor through network outages.
pub async fn run(stack: Stack<'static>) -> ! {
    clock_log!(
        info,
        "NTP client configured for {}:{}",
        config::NTP_SERVER_ADDRESS,
        NTP_PORT
    );

    loop {
        stack.wait_config_up().await;
        match request_time(stack).await {
            Ok((result, instant)) => {
                match ntp_timestamp(result.sec(), result.sec_fraction(), result.roundtrip()) {
                    Ok(utc) => {
                        CLOCK.lock(|clock| *clock.borrow_mut() = Some(Anchor { utc, instant }));
                        clock_log!(
                            info,
                            "UTC synchronized: {} (stratum {}, round trip {} us)",
                            utc,
                            result.stratum(),
                            result.roundtrip()
                        );
                        Timer::after(SYNC_INTERVAL).await;
                        continue;
                    }
                    Err(error) => clock_log!(warn, "NTP synchronization failed: {}", error),
                }
            }
            Err(error) => clock_log!(warn, "NTP synchronization failed: {}", error),
        }
        Timer::after(RETRY_INTERVAL).await;
    }
}

async fn request_time(stack: Stack<'static>) -> Result<(NtpResult, Instant), Error> {
    let mut receive_metadata = [PacketMetadata::EMPTY; 1];
    let mut receive_buffer = [0; NTP_PACKET_SIZE];
    let mut transmit_metadata = [PacketMetadata::EMPTY; 1];
    let mut transmit_buffer = [0; NTP_PACKET_SIZE];
    let mut socket = UdpSocket::new(
        stack,
        &mut receive_metadata,
        &mut receive_buffer,
        &mut transmit_metadata,
        &mut transmit_buffer,
    );
    socket.bind(0).map_err(Error::Bind)?;

    let socket = UdpSocketWrapper::new(socket);
    let server = SocketAddr::from((config::NTP_SERVER_ADDRESS, NTP_PORT));
    let context = NtpContext::new(EmbassyTimestampGenerator::default());
    match with_timeout(REQUEST_TIMEOUT, get_time(server, &socket, context)).await {
        // Capture the anchor before conversion/logging, and drop the socket before sleeping.
        Ok(Ok(result)) => Ok((result, Instant::now())),
        Ok(Err(error)) => Err(Error::Sntp(error)),
        Err(_) => Err(Error::RequestTimeout),
    }
}

fn ntp_timestamp(seconds: u64, fraction: u32, roundtrip_micros: u64) -> Result<Timestamp, Error> {
    // Half the round trip in nanoseconds also preserves odd microseconds.
    let delay_nanos = roundtrip_micros
        .checked_mul(500)
        .ok_or(Error::TimestampOutOfRange)?;
    let nanos = u64::from(fraction_to_nanoseconds(fraction))
        .checked_add(delay_nanos)
        .ok_or(Error::TimestampOutOfRange)?;
    let seconds = seconds
        .checked_add(nanos / 1_000_000_000)
        .and_then(|seconds| i64::try_from(seconds).ok())
        .ok_or(Error::TimestampOutOfRange)?;
    let timestamp = Timestamp::new(seconds, (nanos % 1_000_000_000) as i32)
        .map_err(|_| Error::TimestampOutOfRange)?;
    if !valid_timestamp(timestamp) {
        return Err(Error::TimestampOutOfRange);
    }
    Ok(timestamp)
}

fn timestamp_at(anchor: Option<Anchor>, instant: Instant) -> Option<Timestamp> {
    let anchor = anchor?;
    let elapsed = instant.checked_duration_since(anchor.instant)?;
    anchor
        .utc
        .checked_add(core::time::Duration::from_micros(elapsed.as_micros()))
        .ok()
}

fn valid_timestamp(timestamp: Timestamp) -> bool {
    // Validate UTC, not the local year (Brussels can already be in the next year).
    (946_684_800..4_102_444_800).contains(&timestamp.as_second())
}

fn local_datetime(timestamp: Timestamp) -> Option<LocalDateTime> {
    if !valid_timestamp(timestamp) {
        return None;
    }
    let local = timestamp.to_zoned(TIME_ZONE.clone());
    Some(LocalDateTime::new(
        i32::from(local.year()),
        local.month() as u8,
        local.day() as u8,
        local.weekday().to_sunday_zero_offset() as u8,
        local.hour() as u8,
        local.minute() as u8,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timestamp(value: &str) -> Timestamp {
        value.parse().unwrap()
    }

    #[test]
    fn unsynchronized_clock_has_no_time() {
        assert_eq!(timestamp_at(None, Instant::from_secs(60)), None);
    }

    #[test]
    fn anchor_advances_by_elapsed_time_without_another_sync() {
        let anchor = Anchor {
            utc: timestamp("2026-09-09T10:34:59.750Z"),
            instant: Instant::from_secs(100),
        };
        assert_eq!(timestamp_at(Some(anchor), anchor.instant), Some(anchor.utc));
        assert_eq!(
            timestamp_at(Some(anchor), Instant::from_micros(101_500_000)),
            Some(timestamp("2026-09-09T10:35:01.250Z"))
        );
        assert_eq!(
            timestamp_at(Some(anchor), Instant::from_secs(100 + 24 * 60 * 60)),
            Some(timestamp("2026-09-10T10:34:59.750Z"))
        );
        assert_eq!(timestamp_at(Some(anchor), Instant::from_secs(99)), None);
    }

    #[test]
    fn fraction_and_half_roundtrip_carry_into_next_second() {
        let seconds = timestamp("2026-09-09T10:34:59Z").as_second() as u64;
        let corrected = ntp_timestamp(seconds, 1 << 31, 1_000_001).unwrap();
        assert_eq!(corrected.as_second(), seconds as i64 + 1);
        assert_eq!(corrected.subsec_nanosecond(), 500);
    }

    #[test]
    fn brussels_spring_dst_skips_two_oclock() {
        assert_eq!(
            local_datetime(timestamp("2026-03-29T00:59:59Z")),
            Some(LocalDateTime::new(2026, 3, 29, 0, 1, 59))
        );
        assert_eq!(
            local_datetime(timestamp("2026-03-29T01:00:00Z")),
            Some(LocalDateTime::new(2026, 3, 29, 0, 3, 0))
        );
    }

    #[test]
    fn brussels_autumn_dst_repeats_two_oclock() {
        assert_eq!(
            local_datetime(timestamp("2026-10-25T00:59:59Z")),
            Some(LocalDateTime::new(2026, 10, 25, 0, 2, 59))
        );
        assert_eq!(
            local_datetime(timestamp("2026-10-25T01:00:00Z")),
            Some(LocalDateTime::new(2026, 10, 25, 0, 2, 0))
        );
        assert_eq!(
            local_datetime(timestamp("2026-10-25T00:30:00Z")),
            local_datetime(timestamp("2026-10-25T01:30:00Z"))
        );
    }

    #[test]
    fn local_date_year_and_weekday_roll_over() {
        for (utc, expected) in [
            (
                "2026-12-31T22:59:59Z",
                LocalDateTime::new(2026, 12, 31, 4, 23, 59),
            ),
            (
                "2026-12-31T23:00:00Z",
                LocalDateTime::new(2027, 1, 1, 5, 0, 0),
            ),
            (
                "2026-09-12T21:59:59Z",
                LocalDateTime::new(2026, 9, 12, 6, 23, 59),
            ),
            (
                "2026-09-12T22:00:00Z",
                LocalDateTime::new(2026, 9, 13, 0, 0, 0),
            ),
            (
                "2026-09-13T22:00:00Z",
                LocalDateTime::new(2026, 9, 14, 1, 0, 0),
            ),
            (
                "2028-02-28T23:00:00Z",
                LocalDateTime::new(2028, 2, 29, 2, 0, 0),
            ),
        ] {
            assert_eq!(local_datetime(timestamp(utc)), Some(expected), "{utc}");
        }
    }

    #[test]
    fn invalid_server_timestamps_and_arithmetic_overflow_are_rejected() {
        for seconds in [0, 946_684_799, 4_102_444_800, u64::MAX] {
            assert!(matches!(
                ntp_timestamp(seconds, 0, 0),
                Err(Error::TimestampOutOfRange)
            ));
        }
        assert!(ntp_timestamp(946_684_800, 0, 0).is_ok());
        assert!(ntp_timestamp(4_102_444_799, 0, 0).is_ok());
        assert!(ntp_timestamp(4_102_444_799, 1 << 31, 1_000_000).is_err());
        assert!(ntp_timestamp(946_684_800, 0, u64::MAX).is_err());
        assert!(ntp_timestamp(u64::MAX, 0, 2_000_000).is_err());
        assert_eq!(local_datetime(timestamp("1999-12-31T23:59:59Z")), None);
        assert_eq!(local_datetime(timestamp("2100-01-01T00:00:00Z")), None);
    }
}
