use core::{
    fmt::Write as _,
    sync::atomic::{AtomicU32, Ordering},
};

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use log::{Level, Log, Metadata, Record};

const LOG_MESSAGE_CAPACITY: usize = 512;
pub const STRUCTURED_LOG_MESSAGE_CAPACITY: usize = 1536;
// Structured logging is limited to warnings and errors, so one pending record is enough.
const STRUCTURED_LOG_QUEUE_CAPACITY: usize = 1;
const STRUCTURED_LOG_APP_NAME: &str = "energydisplay";
const STRUCTURED_LOG_HOSTNAME: &str = "energydisplay";

pub type StructuredLogMessage = heapless::String<STRUCTURED_LOG_MESSAGE_CAPACITY>;

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
            let mut rendered = heapless::String::<LOG_MESSAGE_CAPACITY>::new();
            let _ = write!(rendered, "{}", record.args());
            let structured = format_structured_log_message(record, rendered.as_str());
            if STRUCTURED_LOG_QUEUE.try_send(structured).is_err() {
                STRUCTURED_LOG_DROPPED.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn flush(&self) {}
}

/// Installs the logger that mirrors every application record to serial and to
/// the structured-log forwarding queue.
pub fn initialize() {
    if log::set_logger(&LOGGER).is_err() {
        panic!("global logger already initialized");
    }
    log::set_max_level(log::LevelFilter::Info);
}

pub async fn next_structured_log_message() -> StructuredLogMessage {
    STRUCTURED_LOG_QUEUE.receive().await
}

fn serial(record: &Record<'_>) {
    #[cfg(target_arch = "xtensa")]
    esp_println::println!("{}", record.args());

    #[cfg(not(target_arch = "xtensa"))]
    let _ = record;
}

/// Writes an internal forwarding diagnostic to serial without feeding it back
/// into the structured-log queue.
pub fn serial_only(level: Level, arguments: core::fmt::Arguments<'_>) {
    let record = Record::builder()
        .args(arguments)
        .level(level)
        .target("logging")
        .build();
    serial(&record);
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

fn json_string<const N: usize>(output: &mut heapless::String<N>, value: &str) {
    output.push('"').ok();
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\"").ok(),
            '\\' => output.push_str("\\\\").ok(),
            '\n' => output.push_str("\\n").ok(),
            '\r' => output.push_str("\\r").ok(),
            '\t' => output.push_str("\\t").ok(),
            character if character <= '\u{1f}' => {
                write!(output, "\\u{:04x}", character as u32).ok()
            }
            character => output.push(character).ok(),
        };
    }
    output.push('"').ok();
}

fn format_structured_log_message(record: &Record<'_>, message: &str) -> StructuredLogMessage {
    let mut output = StructuredLogMessage::new();
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
    json_string(&mut output, message);
    output.push_str(",\"_msg\":").ok();
    json_string(&mut output, message);
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
