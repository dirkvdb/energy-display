use std::{collections::BTreeMap, env, fs, path::PathBuf, sync::Arc};

use cosmic_text::{
    Align, Attrs, Buffer, CacheKey, CacheKeyFlags, Family, FontSystem, Metrics, Shaping,
    SwashCache, Weight, Wrap,
    fontdb::{ID, Source},
};

const TEXT_FAMILY: &str = "Bitter";
const ICON_FAMILY: &str = "Material Design Icons";
const TEXT_CHARS: &str = " %-.0123456789:?CNOPWabcdefghijklmnopqrstuvwxyz°↑↓";
const TEXT_SIZES: &[f32] = &[18.0, 23.0, 29.0, 32.0, 43.0];
const ICONS: &[(char, f32)] = &[
    ('\u{f139d}', 32.0 / 1.5),
    ('\u{f140b}', 45.0),
    ('\u{f1a74}', 45.0),
    ('\u{f1aaf}', 32.0),
    ('\u{f09a0}', 32.0),
];
const X_OFFSETS: &[f32] = &[0.0, 0.25, 0.5, 0.75];

#[derive(Clone, Copy)]
struct ShapedGlyph {
    font_id: ID,
    glyph_id: u16,
    font_weight: cosmic_text::fontdb::Weight,
    advance: f32,
    x_offset: f32,
    y_offset: f32,
}

struct GeneratedBitmap {
    left: i16,
    top: i16,
    width: u8,
    height: u8,
    data_start: u32,
}

fn main() {
    println!("cargo:rerun-if-env-changed=BITTER_BLACK_FONT");
    println!("cargo:rerun-if-env-changed=MATERIAL_DESIGN_ICONS_FONT");

    let bitter_path = required_path("BITTER_BLACK_FONT");
    let icons_path = required_path("MATERIAL_DESIGN_ICONS_FONT");
    println!("cargo:rerun-if-changed={}", bitter_path.display());
    println!("cargo:rerun-if-changed={}", icons_path.display());

    let bitter = fs::read(&bitter_path).expect("failed to read BITTER_BLACK_FONT");
    let icons = fs::read(&icons_path).expect("failed to read MATERIAL_DESIGN_ICONS_FONT");
    let mut font_system = FontSystem::new_with_fonts([
        Source::Binary(Arc::new(bitter)),
        Source::Binary(Arc::new(icons)),
    ]);
    let mut cache = SwashCache::new();
    let mut bitmap_data = Vec::new();
    let mut generated = String::new();

    for size in TEXT_SIZES {
        generate_font(
            &mut generated,
            &mut bitmap_data,
            &mut font_system,
            &mut cache,
            "TEXT",
            TEXT_FAMILY,
            Weight::BLACK,
            *size,
            TEXT_CHARS,
            true,
        );
    }

    let mut icons_by_size = BTreeMap::<u32, String>::new();
    for &(icon, size) in ICONS {
        icons_by_size.entry(size.to_bits()).or_default().push(icon);
    }
    for (size_bits, icons) in &icons_by_size {
        generate_font(
            &mut generated,
            &mut bitmap_data,
            &mut font_system,
            &mut cache,
            "ICON",
            ICON_FAMILY,
            Weight::NORMAL,
            f32::from_bits(*size_bits),
            icons,
            false,
        );
    }

    generated.push_str("\nstatic BITMAP_DATA: &[u8] = &[\n");
    for chunk in bitmap_data.chunks(24) {
        generated.push_str("    ");
        for byte in chunk {
            generated.push_str(&format!("0x{byte:02x}, "));
        }
        generated.push('\n');
    }
    generated.push_str("];\n\nstatic TEXT_FONTS: &[BitmapFont] = &[\n");
    for size in TEXT_SIZES {
        let suffix = suffix(*size);
        let baseline = baseline(&mut font_system, "0", TEXT_FAMILY, Weight::BLACK, *size);
        generated.push_str(&format!(
            "    BitmapFont {{ size_bits: 0x{:08x}, baseline: {baseline}, glyphs: TEXT_GLYPHS_{suffix}, pairs: TEXT_PAIRS_{suffix} }},\n",
            size.to_bits()
        ));
    }
    generated.push_str("];\n\nstatic ICON_FONTS: &[BitmapFont] = &[\n");
    for (size_bits, icons) in &icons_by_size {
        let size = f32::from_bits(*size_bits);
        let suffix = suffix(size);
        let baseline = baseline(
            &mut font_system,
            &icons.chars().next().unwrap().to_string(),
            ICON_FAMILY,
            Weight::NORMAL,
            size,
        );
        generated.push_str(&format!(
            "    BitmapFont {{ size_bits: 0x{size_bits:08x}, baseline: {baseline}, glyphs: ICON_GLYPHS_{suffix}, pairs: &[] }},\n"
        ));
    }
    generated.push_str("];\n");

    let output =
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is not set")).join("font_data.rs");
    fs::write(output, generated).expect("failed to write generated font data");
}

fn required_path(name: &str) -> PathBuf {
    env::var_os(name).map(PathBuf::from).unwrap_or_else(|| {
        panic!("{name} is not set; enter the repository's devenv shell before building")
    })
}

#[allow(clippy::too_many_arguments)]
fn generate_font(
    output: &mut String,
    bitmap_data: &mut Vec<u8>,
    font_system: &mut FontSystem,
    cache: &mut SwashCache,
    prefix: &str,
    family: &'static str,
    weight: Weight,
    size: f32,
    characters: &str,
    generate_pairs: bool,
) {
    let suffix = suffix(size);
    let glyph_name = format!("{prefix}_GLYPHS_{suffix}");
    let pair_name = format!("{prefix}_PAIRS_{suffix}");
    let mut chars = characters.chars().collect::<Vec<_>>();
    chars.sort_unstable();
    let mut shaped = Vec::with_capacity(chars.len());

    output.push_str(&format!("static {glyph_name}: &[BitmapGlyph] = &[\n"));
    for &character in &chars {
        let glyph = shape(font_system, &character.to_string(), family, weight, size)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("{family} has no glyph for {character:?} at {size}px"));
        assert_eq!(glyph.x_offset, 0.0, "unexpected x offset for {character:?}");
        assert_eq!(glyph.y_offset, 0.0, "unexpected y offset for {character:?}");
        shaped.push(glyph);

        let mut bitmaps = Vec::with_capacity(4);
        for &x_offset in X_OFFSETS {
            let (key, _, _) = CacheKey::new(
                glyph.font_id,
                glyph.glyph_id,
                size,
                (x_offset, 0.0),
                glyph.font_weight,
                CacheKeyFlags::DISABLE_HINTING,
            );
            let image = cache
                .get_image_uncached(font_system, key)
                .unwrap_or_else(|| panic!("failed to rasterize {character:?} at {size}px"));
            bitmaps.push(threshold_bitmap(&image, bitmap_data));
        }

        output.push_str(&format!(
            "    BitmapGlyph {{ character: {character:?}, advance: f32::from_bits(0x{:08x}), bitmaps: [",
            glyph.advance.to_bits()
        ));
        for bitmap in bitmaps {
            output.push_str(&format!(
                "Bitmap {{ left: {}, top: {}, width: {}, height: {}, data_start: {} }}, ",
                bitmap.left, bitmap.top, bitmap.width, bitmap.height, bitmap.data_start
            ));
        }
        output.push_str("] },\n");
    }
    output.push_str("];\n");

    if generate_pairs {
        output.push_str(&format!("static {pair_name}: &[PairAdvance] = &[\n"));
        for (left_index, &left) in chars.iter().enumerate() {
            for &right in &chars {
                let pair = format!("{left}{right}");
                let pair_glyphs = shape(font_system, &pair, family, weight, size);
                if pair_glyphs.len() != 2 || pair_glyphs[0].glyph_id != shaped[left_index].glyph_id
                {
                    continue;
                }
                let advance = pair_glyphs[0].advance;
                if advance.to_bits() != shaped[left_index].advance.to_bits() {
                    let key = (u64::from(left as u32) << 32) | u64::from(right as u32);
                    output.push_str(&format!(
                        "    PairAdvance {{ key: 0x{key:016x}, advance: f32::from_bits(0x{:08x}) }},\n",
                        advance.to_bits()
                    ));
                }
            }
        }
        output.push_str("];\n\n");
    }
}

fn baseline(
    font_system: &mut FontSystem,
    text: &str,
    family: &'static str,
    weight: Weight,
    size: f32,
) -> i16 {
    let mut buffer = Buffer::new(font_system, Metrics::relative(size, 1.0));
    let mut buffer = buffer.borrow_with(font_system);
    buffer.set_size(Some(4096.0), None);
    buffer.set_wrap(Wrap::None);
    let attrs = Attrs::new()
        .family(Family::Name(family))
        .weight(weight)
        .cache_key_flags(CacheKeyFlags::DISABLE_HINTING);
    buffer.set_text(text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(true);
    buffer.layout_runs().next().unwrap().line_y as i16
}

fn shape(
    font_system: &mut FontSystem,
    text: &str,
    family: &'static str,
    weight: Weight,
    size: f32,
) -> Vec<ShapedGlyph> {
    let mut buffer = Buffer::new(font_system, Metrics::relative(size, 1.0));
    let mut buffer = buffer.borrow_with(font_system);
    buffer.set_size(Some(4096.0), None);
    buffer.set_wrap(Wrap::None);
    let attrs = Attrs::new()
        .family(Family::Name(family))
        .weight(weight)
        .cache_key_flags(CacheKeyFlags::DISABLE_HINTING);
    buffer.set_text(text, &attrs, Shaping::Advanced, None);
    for line in &mut buffer.lines {
        line.set_align(Some(Align::Left));
    }
    buffer.shape_until_scroll(true);
    buffer
        .layout_runs()
        .flat_map(|run| run.glyphs)
        .map(|glyph| ShapedGlyph {
            font_id: glyph.font_id,
            glyph_id: glyph.glyph_id,
            font_weight: glyph.font_weight,
            advance: glyph.w,
            x_offset: glyph.x_offset,
            y_offset: glyph.y_offset,
        })
        .collect()
}

fn threshold_bitmap(image: &cosmic_text::SwashImage, bitmap_data: &mut Vec<u8>) -> GeneratedBitmap {
    assert_eq!(
        image.content,
        cosmic_text::SwashContent::Mask,
        "fonts must rasterize to alpha masks"
    );
    let source_width = usize::try_from(image.placement.width).expect("glyph width exceeds usize");
    let source_height =
        usize::try_from(image.placement.height).expect("glyph height exceeds usize");
    let mut min_x = source_width;
    let mut min_y = source_height;
    let mut max_x = 0;
    let mut max_y = 0;
    let mut has_ink = false;
    for y in 0..source_height {
        for x in 0..source_width {
            if image.data[y * source_width + x] > 127 {
                has_ink = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    let data_start = u32::try_from(bitmap_data.len()).expect("generated bitmap data exceeds 4 GiB");
    if !has_ink {
        return GeneratedBitmap {
            left: 0,
            top: 0,
            width: 0,
            height: 0,
            data_start,
        };
    }

    let width = max_x - min_x + 1;
    let height = max_y - min_y + 1;
    let packed_len = (width * height).div_ceil(8);
    bitmap_data.resize(bitmap_data.len() + packed_len, 0);
    for y in 0..height {
        for x in 0..width {
            if image.data[(min_y + y) * source_width + min_x + x] > 127 {
                let bit = y * width + x;
                bitmap_data[data_start as usize + bit / 8] |= 1 << (bit & 7);
            }
        }
    }

    GeneratedBitmap {
        left: i16::try_from(image.placement.left).expect("glyph left placement exceeds i16")
            + i16::try_from(min_x).expect("glyph width exceeds i16"),
        top: -i16::try_from(image.placement.top).expect("glyph top placement exceeds i16")
            + i16::try_from(min_y).expect("glyph height exceeds i16"),
        width: u8::try_from(width).expect("glyph width exceeds 255px"),
        height: u8::try_from(height).expect("glyph height exceeds 255px"),
        data_start,
    }
}

fn suffix(size: f32) -> String {
    format!("{:08X}", size.to_bits())
}
