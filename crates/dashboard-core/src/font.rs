use alloc::{sync::Arc, vec::Vec};

use cosmic_text::{
    Align, Attrs, Buffer, Color, FontSystem, Metrics, Shaping, SwashCache, Weight, fontdb::Source,
};
use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, Pixel, Point},
    primitives::Rectangle,
    text::Alignment,
};

const BITTER_PRO_BLACK: &[u8; 269_356] =
    include_bytes!("../../../assets/fonts/BitterPro-Black.otf");
const MATERIAL_DESIGN_ICONS: &[u8] = include_bytes!(env!("MATERIAL_DESIGN_ICONS_FONT"));
const TEXT_FONT_FAMILY: &str = "Bitter Pro";
const ICON_FONT_FAMILY: &str = "Material Design Icons";

#[derive(Debug)]
pub(crate) struct FontRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
}

impl FontRenderer {
    pub(crate) fn new() -> Self {
        let font_system = FontSystem::new_with_fonts([
            Source::Binary(Arc::new(BITTER_PRO_BLACK)),
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

        let attrs = Attrs::new()
            .family(cosmic_text::Family::Name(family))
            .weight(weight);
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
        let mut pixels = Vec::new();
        buffer.draw(
            &mut self.swash_cache,
            Color::rgb(0, 0, 0),
            |x, y, _width, _height, glyph_color| {
                if glyph_color.a() > 127 {
                    let point = bounds.top_left + Point::new(x, y);
                    min_y = min_y.min(point.y);
                    max_y = max_y.max(point.y);
                    pixels.push(Pixel(point, color));
                }
            },
        );

        if pixels.is_empty() {
            return Ok(());
        }

        let top_distance = min_y - bounds.top_left.y;
        let bottom = bounds.top_left.y + bounds.size.height as i32 - 1;
        let bottom_distance = bottom - max_y;
        let vertical_offset = (bottom_distance - top_distance) / 2;
        for pixel in &mut pixels {
            pixel.0.y += vertical_offset;
        }

        display.draw_iter(pixels)
    }
}
