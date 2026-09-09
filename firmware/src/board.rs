//! Waveshare ESP32-S3-RLCD-4.2 board constants.

pub const DISPLAY_WIDTH: u32 = 400;
pub const DISPLAY_HEIGHT: u32 = 300;
pub const DISPLAY_NATIVE_WIDTH: u32 = 300;
pub const DISPLAY_NATIVE_HEIGHT: u32 = 400;

pub const DISPLAY_SPI_FREQUENCY_MHZ: u32 = 10;
pub const DISPLAY_SPI_MODE: u8 = 0;
pub const DISPLAY_SCLK_GPIO: u8 = 11;
pub const DISPLAY_MOSI_GPIO: u8 = 12;
pub const DISPLAY_DC_GPIO: u8 = 5;
pub const DISPLAY_CS_GPIO: u8 = 40;
pub const DISPLAY_RESET_GPIO: u8 = 41;
pub const DISPLAY_TE_GPIO: u8 = 6;

/// Regular internal RAM reserved for `cosmic-text`, glyph caches, and radio allocations.
pub const FONT_HEAP_SIZE: usize = 216 * 1024;
/// Boot-time memory reclaimed for the Wi-Fi driver and network allocations.
pub const RADIO_HEAP_SIZE: usize = 64 * 1024;

pub const HEARTBEAT_INTERVAL_SECS: u64 = 5;
