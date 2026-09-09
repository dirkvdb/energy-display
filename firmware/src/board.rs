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

/// Internal RAM reserved for `cosmic-text` shaping and glyph caches.
pub const FONT_HEAP_SIZE: usize = 256 * 1024;

pub const HEARTBEAT_INTERVAL_SECS: u64 = 5;
