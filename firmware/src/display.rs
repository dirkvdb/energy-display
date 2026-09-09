#[cfg(target_arch = "xtensa")]
use display_interface::WriteOnlyDataCommand;
use embedded_graphics::{
    mono_font::{
        MonoTextStyleBuilder,
        ascii::{FONT_6X10, FONT_10X20},
    },
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};
#[cfg(target_arch = "xtensa")]
use embedded_hal::digital::OutputPin;
#[cfg(target_arch = "xtensa")]
use st7305::St7305;

use crate::board::{DISPLAY_HEIGHT, DISPLAY_WIDTH};

/// Physical black on the Waveshare ST7305 panel profile.
pub const BLACK: BinaryColor = BinaryColor::Off;
/// Physical white on the Waveshare ST7305 panel profile.
pub const WHITE: BinaryColor = BinaryColor::On;

#[cfg(target_arch = "xtensa")]
const WHITE_FRAMEBUFFER_BYTE: u8 = 0xff;

/// Clears the driver's packed framebuffer without drawing 120,000 pixels.
#[cfg(target_arch = "xtensa")]
pub fn clear_white<DI, RST>(display: &mut St7305<DI, RST>)
where
    DI: WriteOnlyDataCommand,
    RST: OutputPin,
{
    display.color_clear(WHITE_FRAMEBUFFER_BYTE);
}

pub fn draw_hardware_check<D>(display: &mut D) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let center_x = (DISPLAY_WIDTH / 2) as i32;
    let title_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(BLACK)
        .background_color(WHITE)
        .build();
    let detail_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BLACK)
        .background_color(WHITE)
        .build();

    Rectangle::new(
        Point::new(1, 1),
        Size::new(DISPLAY_WIDTH - 2, DISPLAY_HEIGHT - 2),
    )
    .into_styled(PrimitiveStyle::with_stroke(BLACK, 2))
    .draw(display)?;

    Text::with_alignment(
        "ENERGY DISPLAY",
        Point::new(center_x, 132),
        title_style,
        Alignment::Center,
    )
    .draw(display)?;
    Text::with_alignment(
        "ESP32-S3 + ST7305 OK",
        Point::new(center_x, 162),
        title_style,
        Alignment::Center,
    )
    .draw(display)?;
    Text::with_alignment(
        "400x300 landscape / SPI 10 MHz",
        Point::new(center_x, 188),
        detail_style,
        Alignment::Center,
    )
    .draw(display)?;

    Ok(())
}
