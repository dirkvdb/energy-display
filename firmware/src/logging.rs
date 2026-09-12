use core::{
    fmt::{self, Write as _},
    sync::atomic::{AtomicU32, Ordering},
};

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use log::{Level, Log, Metadata, Record};

pub const STRUCTURED_LOG_MESSAGE_CAPACITY: usize = 1024;
// Structured logging is limited to warnings and errors, so one pending record is enough.
const STRUCTURED_LOG_QUEUE_CAPACITY: usize = 1;
const STRUCTURED_LOG_APP_NAME: &str = "energydisplay";
const STRUCTURED_LOG_HOSTNAME: &str = "energydisplay";

pub struct StructuredLogMessage {
    json: heapless::String<STRUCTURED_LOG_MESSAGE_CAPACITY>,
    persisted_panic: bool,
}

impl StructuredLogMessage {
    pub fn empty() -> Self {
        Self {
            json: heapless::String::new(),
            persisted_panic: false,
        }
    }

    pub fn is_persisted_panic(&self) -> bool {
        self.persisted_panic
    }
}

impl fmt::Display for StructuredLogMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.json)
    }
}

static STRUCTURED_LOG_QUEUE: Channel<
    CriticalSectionRawMutex,
    StructuredLogMessage,
    STRUCTURED_LOG_QUEUE_CAPACITY,
> = Channel::new();
static STRUCTURED_LOG_DROPPED: AtomicU32 = AtomicU32::new(0);
static LOGGER: FanoutLogger = FanoutLogger;

struct FanoutLogger;

impl Log for FanoutLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }

        serial(record);

        if record.level() <= Level::Warn {
            enqueue_structured(record);
        }
    }

    fn flush(&self) {}
}

/// Installs the logger that mirrors every application record to serial and
/// queues warning/error records for structured forwarding.
pub fn initialize() {
    if log::set_logger(&LOGGER).is_err() {
        panic!("global logger already initialized");
    }
    log::set_max_level(log::LevelFilter::Info);
}

pub async fn next_structured_log_message() -> StructuredLogMessage {
    STRUCTURED_LOG_QUEUE.receive().await
}

/// Enqueues the panic recovered from flash as the first structured error of
/// this boot. The persistence marker lets the forwarder clear flash only after
/// Victoria Logs acknowledges the record.
pub fn report_previous_panic(message: &str) {
    report_previous_panic_arguments(format_args!("previous boot panicked: {message}"));
}

fn report_previous_panic_arguments(arguments: fmt::Arguments<'_>) {
    let record = Record::builder()
        .args(arguments)
        .level(Level::Error)
        .target("panic")
        .build();
    serial(&record);
    enqueue_structured_record(&record, true);
}

fn serial(record: &Record<'_>) {
    #[cfg(target_arch = "xtensa")]
    if let Some(timestamp) = crate::clock::utc_now() {
        esp_println::println!("[{}] {}", timestamp, record.args());
    } else {
        let elapsed_ms = esp_hal::time::Instant::now()
            .duration_since_epoch()
            .as_millis();
        esp_println::println!(
            "[+{}.{:03}s] {}",
            elapsed_ms / 1_000,
            elapsed_ms % 1_000,
            record.args()
        );
    }

    #[cfg(not(target_arch = "xtensa"))]
    let _ = record;
}

/// Writes an internal forwarding diagnostic to serial without feeding it back
/// into the structured-log queue.
pub fn serial_only(level: Level, arguments: fmt::Arguments<'_>) {
    let record = Record::builder()
        .args(arguments)
        .level(level)
        .target("logging")
        .build();
    serial(&record);
}

// Keep the structured formatter and its buffer out of the hot path used by
// every serial info record. Inlining this function adds several KiB to each
// MQTT logging call's stack frame on Xtensa.
#[cold]
#[inline(never)]
fn enqueue_structured(record: &Record<'_>) {
    enqueue_structured_record(record, false);
}

fn enqueue_structured_record(record: &Record<'_>, persisted_panic: bool) {
    let structured = StructuredLogMessage {
        json: format_structured_log_message(record),
        persisted_panic,
    };
    if STRUCTURED_LOG_QUEUE.try_send(structured).is_err() {
        STRUCTURED_LOG_DROPPED.fetch_add(1, Ordering::Relaxed);
    }
}

fn level_name(level: Level) -> &'static str {
    match level {
        Level::Error => "error",
        Level::Warn => "warn",
        Level::Info => "info",
        Level::Debug => "debug",
        Level::Trace => "trace",
    }
}

struct JsonWriter<'a, const N: usize> {
    output: &'a mut heapless::String<N>,
}

impl<const N: usize> fmt::Write for JsonWriter<'_, N> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        for character in value.chars() {
            let result = match character {
                '"' => self.output.push_str("\\\""),
                '\\' => self.output.push_str("\\\\"),
                '\n' => self.output.push_str("\\n"),
                '\r' => self.output.push_str("\\r"),
                '\t' => self.output.push_str("\\t"),
                character if character <= '\u{1f}' => {
                    write!(self.output, "\\u{:04x}", character as u32)?;
                    continue;
                }
                character => self.output.push(character),
            };
            result.map_err(|_| fmt::Error)?;
        }
        Ok(())
    }
}

fn json_string<const N: usize>(output: &mut heapless::String<N>, value: &str) {
    output.push('"').ok();
    JsonWriter { output }.write_str(value).ok();
    output.push('"').ok();
}

fn json_arguments<const N: usize>(output: &mut heapless::String<N>, value: &fmt::Arguments<'_>) {
    output.push('"').ok();
    fmt::write(&mut JsonWriter { output }, value.clone()).ok();
    output.push('"').ok();
}

fn format_structured_log_message(
    record: &Record<'_>,
) -> heapless::String<STRUCTURED_LOG_MESSAGE_CAPACITY> {
    let mut output = heapless::String::new();
    output.push('{').ok();
    output.push_str("\"app_name\":").ok();
    json_string(&mut output, STRUCTURED_LOG_APP_NAME);
    output.push_str(",\"hostname\":").ok();
    json_string(&mut output, STRUCTURED_LOG_HOSTNAME);
    output
        .push_str(",\"proc_id\":\"1\",\"device\":\"energy-display\",\"level\":")
        .ok();
    json_string(&mut output, level_name(record.level()));
    output.push_str(",\"component\":").ok();
    json_string(
        &mut output,
        record
            .target()
            .rsplit("::")
            .next()
            .unwrap_or(record.target()),
    );
    output.push_str(",\"message\":").ok();
    json_arguments(&mut output, record.args());
    output.push_str(",\"_msg\":").ok();
    json_arguments(&mut output, record.args());
    output.push_str(",\"target\":").ok();
    json_string(&mut output, record.target());
    if let Some(module_path) = record.module_path() {
        output.push_str(",\"module_path\":").ok();
        json_string(&mut output, module_path);
    }
    if let Some(file) = record.file() {
        output.push_str(",\"file\":").ok();
        json_string(&mut output, file);
    }
    if let Some(line) = record.line() {
        write!(output, ",\"line\":{line}").ok();
    }
    output.push('}').ok();
    output
}
