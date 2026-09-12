use std::{
    convert::Infallible,
    hint::black_box,
    time::{Duration, Instant},
};

use dashboard_core::{
    DashboardRenderer, LocalDateTime,
    model::test_dashboard,
    renderer::{HEIGHT, WHITE, WIDTH},
};
use embedded_graphics::{
    Pixel,
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, OriginDimensions, Size},
};

const FRAMEBUFFER_BYTES: usize = (WIDTH * HEIGHT / 8) as usize;
const DEFAULT_SAMPLES: usize = 50;
const FIXTURE_TIME: LocalDateTime = LocalDateTime::new(2026, 9, 9, 3, 12, 34);

struct St7305Framebuffer {
    buffer: [u8; FRAMEBUFFER_BYTES],
    draw_calls: usize,
    pixels: usize,
}

impl St7305Framebuffer {
    fn new() -> Self {
        Self {
            buffer: [0xff; FRAMEBUFFER_BYTES],
            draw_calls: 0,
            pixels: 0,
        }
    }

    fn reset_metrics(&mut self) {
        self.draw_calls = 0;
        self.pixels = 0;
    }

    fn checksum(&self) -> u64 {
        self.buffer
            .iter()
            .fold(0_u64, |sum, byte| sum.wrapping_add(u64::from(*byte)))
    }
}

impl OriginDimensions for St7305Framebuffer {
    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }
}

impl DrawTarget for St7305Framebuffer {
    type Color = BinaryColor;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        self.draw_calls += 1;
        for Pixel(point, color) in pixels {
            if !(0..WIDTH as i32).contains(&point.x) || !(0..HEIGHT as i32).contains(&point.y) {
                continue;
            }

            self.pixels += 1;
            let x = point.x as usize;
            let inverted_y = HEIGHT as usize - 1 - point.y as usize;
            let index = x / 2 * (HEIGHT as usize / 4) + inverted_y / 4;
            let bit = 7 - (inverted_y % 4 * 2 + x % 2);
            if color.is_on() {
                self.buffer[index] |= 1 << bit;
            } else {
                self.buffer[index] &= !(1 << bit);
            }
        }
        Ok(())
    }
}

fn render_frame(display: &mut St7305Framebuffer, renderer: &mut DashboardRenderer) {
    display.clear(WHITE).unwrap();
    renderer
        .render(display, &test_dashboard().status, FIXTURE_TIME)
        .unwrap();
    black_box(display.checksum());
}

fn main() {
    let samples = std::env::var("RENDER_BENCH_SAMPLES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_SAMPLES);
    assert!(
        samples > 0,
        "RENDER_BENCH_SAMPLES must be greater than zero"
    );

    let mut display = St7305Framebuffer::new();
    let mut renderer = DashboardRenderer::new();
    let cold_start = Instant::now();
    render_frame(&mut display, &mut renderer);
    let cold = cold_start.elapsed();
    let draw_calls = display.draw_calls;
    let pixels = display.pixels;

    let mut samples_elapsed = Vec::with_capacity(samples);
    for _ in 0..samples {
        display.reset_metrics();
        let start = Instant::now();
        render_frame(&mut display, &mut renderer);
        samples_elapsed.push(start.elapsed());
    }
    samples_elapsed.sort_unstable();

    let total: Duration = samples_elapsed.iter().sum();
    let median = samples_elapsed[samples / 2];
    println!("dashboard render ({samples} steady-state samples)");
    println!("  cold:   {:>9.3} ms", cold.as_secs_f64() * 1_000.0);
    println!(
        "  mean:   {:>9.3} ms",
        total.as_secs_f64() * 1_000.0 / samples as f64
    );
    println!("  median: {:>9.3} ms", median.as_secs_f64() * 1_000.0);
    println!("  calls:  {draw_calls:>9} draw_iter calls/frame");
    println!("  pixels: {pixels:>9} pixels/frame");
}
