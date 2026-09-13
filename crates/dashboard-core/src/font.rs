use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, Pixel, Point},
    primitives::Rectangle,
    text::Alignment,
};

#[derive(Clone, Copy, Debug)]
struct Bitmap {
    left: i16,
    top: i16,
    width: u8,
    height: u8,
    data_start: u32,
}

#[derive(Debug)]
struct BitmapGlyph {
    character: char,
    advance: f32,
    bitmaps: [Bitmap; 4],
}

#[derive(Debug)]
struct PairAdvance {
    key: u64,
    advance: f32,
}

#[derive(Debug)]
struct BitmapFont {
    size_bits: u32,
    baseline: i16,
    glyphs: &'static [BitmapGlyph],
    pairs: &'static [PairAdvance],
}

#[derive(Clone, Copy, Debug)]
struct SourceGlyph {
    character: char,
    glyph: &'static BitmapGlyph,
    advance: f32,
}

#[derive(Clone, Copy, Debug)]
struct Word {
    start: usize,
    end: usize,
    width: f32,
    blank: bool,
}

#[derive(Clone, Copy, Debug)]
struct Line {
    start: usize,
    end: usize,
    width: f32,
}

const MAX_LAYOUT_GLYPHS: usize = 64;

include!(concat!(env!("OUT_DIR"), "/font_data.rs"));

#[derive(Debug, Default)]
pub(crate) struct FontRenderer;

impl FontRenderer {
    pub(crate) const fn new() -> Self {
        Self
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
        draw(
            display, text, bounds, color, font_size, alignment, TEXT_FONTS,
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
        draw(
            display,
            icon,
            bounds,
            color,
            font_size,
            Alignment::Center,
            ICON_FONTS,
        )
    }
}

fn draw<D>(
    display: &mut D,
    text: &str,
    bounds: Rectangle,
    color: BinaryColor,
    font_size: f32,
    alignment: Alignment,
    fonts: &'static [BitmapFont],
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = BinaryColor>,
{
    let Some(font) = fonts
        .iter()
        .find(|font| font.size_bits == font_size.to_bits())
    else {
        return Ok(());
    };

    let Some((glyphs, lines)) = layout(text, font, bounds.size.width as f32) else {
        return Ok(());
    };

    let mut min_y = i32::MAX;
    let mut max_y = i32::MIN;
    visit_layout(
        &glyphs,
        &lines,
        bounds.size.width as f32,
        font_size,
        i32::from(font.baseline),
        alignment,
        |_origin_x, baseline_y, bitmap| {
            if bitmap.width != 0 {
                let top = baseline_y + i32::from(bitmap.top);
                min_y = min_y.min(top);
                max_y = max_y.max(top + i32::from(bitmap.height) - 1);
            }
        },
    );
    if min_y == i32::MAX {
        return Ok(());
    }

    let vertical_offset = (bounds.size.height as i32 - 1 - min_y - max_y) / 2;
    let mut result = Ok(());
    visit_layout(
        &glyphs,
        &lines,
        bounds.size.width as f32,
        font_size,
        i32::from(font.baseline),
        alignment,
        |origin_x, baseline_y, bitmap| {
            if result.is_err() || bitmap.width == 0 {
                return;
            }
            let width = usize::from(bitmap.width);
            let pixel_count = width * usize::from(bitmap.height);
            let data_start = bitmap.data_start as usize;
            let pixels = (0..pixel_count).filter_map(|index| {
                let byte = BITMAP_DATA[data_start + index / 8];
                if byte & (1 << (index & 7)) == 0 {
                    return None;
                }
                let point = bounds.top_left
                    + Point::new(
                        origin_x + i32::from(bitmap.left) + (index % width) as i32,
                        vertical_offset
                            + baseline_y
                            + i32::from(bitmap.top)
                            + (index / width) as i32,
                    );
                Some(Pixel(point, color))
            });
            result = display.draw_iter(pixels);
        },
    );
    result
}

fn layout(
    text: &str,
    font: &'static BitmapFont,
    max_width: f32,
) -> Option<(
    heapless::Vec<SourceGlyph, MAX_LAYOUT_GLYPHS>,
    heapless::Vec<Line, MAX_LAYOUT_GLYPHS>,
)> {
    let mut glyphs = heapless::Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        let Some(glyph) = glyph(font, character) else {
            continue;
        };
        let advance = chars
            .peek()
            .and_then(|next| pair_advance(font, character, *next))
            .unwrap_or(glyph.advance);
        glyphs
            .push(SourceGlyph {
                character,
                glyph,
                advance,
            })
            .ok()?;
    }

    let mut words = heapless::Vec::<Word, MAX_LAYOUT_GLYPHS>::new();
    let mut start = 0;
    while start < glyphs.len() {
        let blank = glyphs[start].character == ' ';
        let mut end = start + 1;
        if !blank {
            while end < glyphs.len() && glyphs[end].character != ' ' {
                end += 1;
            }
        }
        let width = glyphs[start..end]
            .iter()
            .fold(0.0, |width, glyph| width + glyph.advance);
        words
            .push(Word {
                start,
                end,
                width,
                blank,
            })
            .ok()?;
        start = end;
    }

    let mut lines = heapless::Vec::<Line, MAX_LAYOUT_GLYPHS>::new();
    let mut range_start = 0;
    let mut range_width = 0.0;
    let mut width_before_last_blank = 0.0;
    for (word_index, word) in words.iter().enumerate() {
        if range_width + word.width <= max_width || (word.blank && range_width <= max_width) {
            if word.blank {
                width_before_last_blank = range_width;
            }
            range_width += word.width;
        } else if word.width > max_width {
            if range_width > 0.0 {
                lines
                    .push(Line {
                        start: range_start,
                        end: word.start,
                        width: range_width,
                    })
                    .ok()?;
                range_start = word.start;
                range_width = 0.0;
            }
            for glyph_index in word.start..word.end {
                let glyph_width = glyphs[glyph_index].advance;
                if range_width + glyph_width <= max_width {
                    range_width += glyph_width;
                } else {
                    if range_start < glyph_index {
                        lines
                            .push(Line {
                                start: range_start,
                                end: glyph_index,
                                width: range_width,
                            })
                            .ok()?;
                    }
                    range_start = glyph_index;
                    range_width = glyph_width;
                }
            }
        } else {
            if range_width > 0.0 {
                let previous_blank = word_index > 0 && words[word_index - 1].blank;
                let (end, width) = if previous_blank {
                    (words[word_index - 1].start, width_before_last_blank)
                } else {
                    (word.start, range_width)
                };
                if range_start < end {
                    lines
                        .push(Line {
                            start: range_start,
                            end,
                            width,
                        })
                        .ok()?;
                }
            }
            range_start = word.start;
            range_width = if word.blank { 0.0 } else { word.width };
        }
    }
    if range_start < glyphs.len() && range_width > 0.0 {
        lines
            .push(Line {
                start: range_start,
                end: glyphs.len(),
                width: range_width,
            })
            .ok()?;
    }
    Some((glyphs, lines))
}

fn visit_layout(
    glyphs: &[SourceGlyph],
    lines: &[Line],
    max_width: f32,
    font_size: f32,
    first_baseline: i32,
    alignment: Alignment,
    mut visitor: impl FnMut(i32, i32, Bitmap),
) {
    for (line_index, line) in lines.iter().enumerate() {
        let remaining = max_width - line.width;
        let mut x = if remaining > 0.0 {
            match alignment {
                Alignment::Left => 0.0,
                Alignment::Center => remaining / 2.0,
                Alignment::Right => remaining,
            }
        } else {
            0.0
        };
        let baseline_y = first_baseline + line_index as i32 * font_size as i32;
        for source in &glyphs[line.start..line.end] {
            let (origin_x, bin) = subpixel_bin(x);
            visitor(origin_x, baseline_y, source.glyph.bitmaps[bin]);
            x += source.advance;
        }
    }
}

fn glyph(font: &'static BitmapFont, character: char) -> Option<&'static BitmapGlyph> {
    font.glyphs
        .binary_search_by_key(&character, |glyph| glyph.character)
        .ok()
        .map(|index| &font.glyphs[index])
}

fn pair_advance(font: &BitmapFont, left: char, right: char) -> Option<f32> {
    let key = (u64::from(left as u32) << 32) | u64::from(right as u32);
    font.pairs
        .binary_search_by_key(&key, |pair| pair.key)
        .ok()
        .map(|index| font.pairs[index].advance)
}

fn subpixel_bin(position: f32) -> (i32, usize) {
    let truncated = position as i32;
    let fraction = position - truncated as f32;
    if position.is_sign_negative() {
        if fraction > -0.125 {
            (truncated, 0)
        } else if fraction > -0.375 {
            (truncated - 1, 3)
        } else if fraction > -0.625 {
            (truncated - 1, 2)
        } else if fraction > -0.875 {
            (truncated - 1, 1)
        } else {
            (truncated - 1, 0)
        }
    } else if fraction < 0.125 {
        (truncated, 0)
    } else if fraction < 0.375 {
        (truncated, 1)
    } else if fraction < 0.625 {
        (truncated, 2)
    } else if fraction < 0.875 {
        (truncated, 3)
    } else {
        (truncated + 1, 0)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::{sync::Arc, vec::Vec};

    use super::*;
    use cosmic_text::{
        Align, Attrs, Buffer, CacheKeyFlags, Color, Family, FontSystem, Metrics, Shaping,
        SwashCache, Weight, fontdb::Source,
    };
    use embedded_graphics::prelude::Size;

    const CANVAS_WIDTH: usize = 256;
    const CANVAS_HEIGHT: usize = 100;

    struct Canvas([u8; CANVAS_WIDTH * CANVAS_HEIGHT / 8]);

    impl Canvas {
        const fn new() -> Self {
            Self([0; CANVAS_WIDTH * CANVAS_HEIGHT / 8])
        }
    }

    impl embedded_graphics::geometry::OriginDimensions for Canvas {
        fn size(&self) -> Size {
            Size::new(CANVAS_WIDTH as u32, CANVAS_HEIGHT as u32)
        }
    }

    impl DrawTarget for Canvas {
        type Color = BinaryColor;
        type Error = core::convert::Infallible;

        fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
        where
            I: IntoIterator<Item = Pixel<Self::Color>>,
        {
            for Pixel(point, color) in pixels {
                if !(0..CANVAS_WIDTH as i32).contains(&point.x)
                    || !(0..CANVAS_HEIGHT as i32).contains(&point.y)
                {
                    continue;
                }
                let index = point.y as usize * CANVAS_WIDTH + point.x as usize;
                if color.is_on() {
                    self.0[index / 8] |= 1 << (index & 7);
                } else {
                    self.0[index / 8] &= !(1 << (index & 7));
                }
            }
            Ok(())
        }
    }

    #[test]
    fn generated_rasterization_matches_cosmic_text() {
        for (text, bounds, size, alignment, is_icon) in [
            (
                "0",
                Rectangle::new(Point::new(3, 2), Size::new(64, 64)),
                43.0,
                Alignment::Center,
                false,
            ),
            (
                "↑ 3.0",
                Rectangle::new(Point::new(3, 2), Size::new(124, 45)),
                43.0,
                Alignment::Center,
                false,
            ),
            (
                "↓ 4.4kW",
                Rectangle::new(Point::new(3, 2), Size::new(120, 37)),
                29.0,
                Alignment::Center,
                false,
            ),
            (
                "23°",
                Rectangle::new(Point::new(3, 2), Size::new(81, 38)),
                32.0,
                Alignment::Center,
                false,
            ),
            (
                "wo  9 sep",
                Rectangle::new(Point::new(3, 2), Size::new(200, 25)),
                18.0,
                Alignment::Left,
                false,
            ),
            (
                "12:34",
                Rectangle::new(Point::new(3, 2), Size::new(200, 25)),
                18.0,
                Alignment::Right,
                false,
            ),
            (
                "999.9",
                Rectangle::new(Point::new(3, 2), Size::new(62, 50)),
                32.0,
                Alignment::Center,
                false,
            ),
            (
                "↓ 4.4kW",
                Rectangle::new(Point::new(3, 2), Size::new(60, 37)),
                29.0,
                Alignment::Center,
                false,
            ),
            (
                "Waiting for time",
                Rectangle::new(Point::new(3, 30), Size::new(50, 25)),
                18.0,
                Alignment::Center,
                false,
            ),
            (
                "\u{f140b}",
                Rectangle::new(Point::new(3, 2), Size::new(75, 75)),
                45.0,
                Alignment::Center,
                true,
            ),
        ] {
            let mut generated = Canvas::new();
            let mut renderer = FontRenderer::new();
            if is_icon {
                renderer
                    .draw_icon(&mut generated, text, bounds, BinaryColor::On, size)
                    .unwrap();
            } else {
                renderer
                    .draw_text(
                        &mut generated,
                        text,
                        bounds,
                        BinaryColor::On,
                        size,
                        alignment,
                    )
                    .unwrap();
            }

            let mut reference = Canvas::new();
            reference_draw(&mut reference, text, bounds, size, alignment, is_icon);
            let differences = generated
                .0
                .iter()
                .zip(reference.0.iter())
                .filter(|(generated, reference)| generated != reference)
                .count();

            assert_eq!(differences, 0, "generated pixels differ for {text:?}");
        }
    }

    fn reference_draw(
        display: &mut Canvas,
        text: &str,
        bounds: Rectangle,
        font_size: f32,
        alignment: Alignment,
        is_icon: bool,
    ) {
        let bitter = include_bytes!(env!("BITTER_BLACK_FONT"));
        let icons = include_bytes!(env!("MATERIAL_DESIGN_ICONS_FONT"));
        let mut font_system = FontSystem::new_with_fonts([
            Source::Binary(Arc::new(bitter.as_slice())),
            Source::Binary(Arc::new(icons.as_slice())),
        ]);
        let mut cache = SwashCache::new();
        let mut buffer = Buffer::new(&mut font_system, Metrics::relative(font_size, 1.0));
        let mut buffer = buffer.borrow_with(&mut font_system);
        buffer.set_size(Some(bounds.size.width as f32), None);
        let attrs = Attrs::new()
            .family(Family::Name(if is_icon {
                "Material Design Icons"
            } else {
                "Bitter"
            }))
            .weight(if is_icon {
                Weight::NORMAL
            } else {
                Weight::BLACK
            })
            .cache_key_flags(CacheKeyFlags::DISABLE_HINTING);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        let align = match alignment {
            Alignment::Left => Align::Left,
            Alignment::Center => Align::Center,
            Alignment::Right => Align::Right,
        };
        for line in &mut buffer.lines {
            line.set_align(Some(align));
        }
        buffer.shape_until_scroll(true);

        let mut min_y = i32::MAX;
        let mut max_y = i32::MIN;
        buffer.draw(
            &mut cache,
            Color::rgb(0, 0, 0),
            |_x, y, _width, _height, color| {
                if color.a() > 127 {
                    let y = bounds.top_left.y + y;
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            },
        );
        if min_y == i32::MAX {
            return;
        }
        let top_distance = min_y - bounds.top_left.y;
        let bottom = bounds.top_left.y + bounds.size.height as i32 - 1;
        let bottom_distance = bottom - max_y;
        let vertical_offset = (bottom_distance - top_distance) / 2;
        let mut pixels = Vec::new();
        buffer.draw(
            &mut cache,
            Color::rgb(0, 0, 0),
            |x, y, _width, _height, color| {
                if color.a() > 127 {
                    pixels.push(Pixel(
                        bounds.top_left + Point::new(x, y + vertical_offset),
                        BinaryColor::On,
                    ));
                }
            },
        );
        display.draw_iter(pixels).unwrap();
    }

    #[test]
    fn subpixel_quantization_matches_cosmic_text() {
        assert_eq!(subpixel_bin(0.124), (0, 0));
        assert_eq!(subpixel_bin(0.125), (0, 1));
        assert_eq!(subpixel_bin(0.375), (0, 2));
        assert_eq!(subpixel_bin(0.625), (0, 3));
        assert_eq!(subpixel_bin(0.875), (1, 0));
        assert_eq!(subpixel_bin(-0.125), (-1, 3));
    }
}
