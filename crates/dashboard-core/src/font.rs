use alloc::sync::Arc;

use cosmic_text::{
    Align, Attrs, Buffer, CacheKeyFlags, Color, FontSystem, Metrics, Shaping, SwashCache, Weight,
    fontdb::Source,
};
use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, Pixel, Point},
    primitives::Rectangle,
    text::Alignment,
};

const BITTER_BLACK: &[u8] = include_bytes!(env!("BITTER_BLACK_FONT"));
const MATERIAL_DESIGN_ICONS: &[u8] = include_bytes!(env!("MATERIAL_DESIGN_ICONS_FONT"));
const TEXT_FONT_FAMILY: &str = "Bitter";
const ICON_FONT_FAMILY: &str = "Material Design Icons";
const PIXEL_BATCH_SIZE: usize = 128;

#[derive(Debug)]
pub(crate) struct FontRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
}

impl FontRenderer {
    pub(crate) fn new() -> Self {
        let font_system = FontSystem::new_with_fonts([
            Source::Binary(Arc::new(BITTER_BLACK)),
            Source::Binary(Arc::new(MATERIAL_DESIGN_ICONS)),
        ]);
        Self {
            font_system,
            swash_cache: SwashCache::new(),
        }
    }

    pub(crate) fn draw_text<D>(
        &mut self,
        display: &mut D,
        text: &str,
        bounds: Rectangle,
        color: BinaryColor,
        font_size: f32,
        alignment: Alignment,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        self.draw(
            display,
            text,
            bounds,
            color,
            font_size,
            alignment,
            TEXT_FONT_FAMILY,
            Weight::BLACK,
        )
    }

    pub(crate) fn draw_icon<D>(
        &mut self,
        display: &mut D,
        icon: &str,
        bounds: Rectangle,
        color: BinaryColor,
        font_size: f32,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        self.draw(
            display,
            icon,
            bounds,
            color,
            font_size,
            Alignment::Center,
            ICON_FONT_FAMILY,
            Weight::NORMAL,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn draw<D>(
        &mut self,
        display: &mut D,
        text: &str,
        bounds: Rectangle,
        color: BinaryColor,
        font_size: f32,
        alignment: Alignment,
        family: &'static str,
        weight: Weight,
    ) -> Result<(), D::Error>
    where
        D: DrawTarget<Color = BinaryColor>,
    {
        let metrics = Metrics::relative(font_size, 1.0);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        let mut buffer = buffer.borrow_with(&mut self.font_system);
        buffer.set_size(Some(bounds.size.width as f32), None);

        // Skrifa hinting initialization overflows the ESP32-S3 stack. Use the same
        // unhinted outlines on host and device to preserve simulator parity.
        let attrs = Attrs::new()
            .family(cosmic_text::Family::Name(family))
            .weight(weight)
            .cache_key_flags(CacheKeyFlags::DISABLE_HINTING);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);

        let alignment = match alignment {
            Alignment::Left => Align::Left,
            Alignment::Center => Align::Center,
            Alignment::Right => Align::Right,
        };
        for line in &mut buffer.lines {
            line.set_align(Some(alignment));
        }
        buffer.shape_until_scroll(true);

        let mut min_y = i32::MAX;
        let mut max_y = i32::MIN;
        buffer.draw(
            &mut self.swash_cache,
            Color::rgb(0, 0, 0),
            |_x, y, _width, _height, glyph_color| {
                if glyph_color.a() > 127 {
                    let y = bounds.top_left.y + y;
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            },
        );

        if min_y == i32::MAX {
            return Ok(());
        }

        let top_distance = min_y - bounds.top_left.y;
        let bottom = bounds.top_left.y + bounds.size.height as i32 - 1;
        let bottom_distance = bottom - max_y;
        let vertical_offset = (bottom_distance - top_distance) / 2;
        let mut pixels = heapless::Vec::<Point, PIXEL_BATCH_SIZE>::new();
        let mut draw_error = None;
        buffer.draw(
            &mut self.swash_cache,
            Color::rgb(0, 0, 0),
            |x, y, _width, _height, glyph_color| {
                if glyph_color.a() > 127 && draw_error.is_none() {
                    let point = bounds.top_left + Point::new(x, y + vertical_offset);
                    if pixels.is_full() {
                        if let Err(error) = display
                            .draw_iter(pixels.iter().copied().map(|point| Pixel(point, color)))
                        {
                            draw_error = Some(error);
                            return;
                        }
                        pixels.clear();
                    }
                    pixels.push(point).unwrap();
                }
            },
        );

        match draw_error {
            Some(error) => Err(error),
            None => display.draw_iter(pixels.into_iter().map(|point| Pixel(point, color))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_graphics::{mock_display::MockDisplay, prelude::Size};

    #[test]
    fn text_and_icons_rasterize_without_hinting() {
        let bounds = Rectangle::new(Point::zero(), Size::new(64, 64));
        for is_icon in [false, true] {
            let mut renderer = FontRenderer::new();
            let mut display = MockDisplay::<BinaryColor>::new();
            display.set_allow_overdraw(true);
            if is_icon {
                renderer
                    .draw_icon(&mut display, "\u{f140b}", bounds, BinaryColor::On, 45.0)
                    .unwrap();
            } else {
                renderer
                    .draw_text(
                        &mut display,
                        "0",
                        bounds,
                        BinaryColor::On,
                        43.0,
                        Alignment::Center,
                    )
                    .unwrap();
            }

            assert!(!renderer.swash_cache.image_cache.is_empty());
            for (key, image) in &renderer.swash_cache.image_cache {
                assert!(key.flags.contains(CacheKeyFlags::DISABLE_HINTING));
                let image = image.as_ref().expect("glyph must rasterize");
                assert!(image.placement.width > 0 && image.placement.height > 0);
            }
        }
    }
}
