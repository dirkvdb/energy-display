use dashboard_core::{
    DashboardRenderer, LocalDateTime,
    model::test_dashboard,
    renderer::{HEIGHT, WHITE, WIDTH},
};
use embedded_graphics::{pixelcolor::BinaryColor, prelude::*};
use embedded_graphics_simulator::{
    BinaryColorTheme, OutputSettingsBuilder, SimulatorDisplay, Window,
};

const FIXTURE_TIME: LocalDateTime = LocalDateTime::new(2026, 9, 9, 3, 12, 34);

fn main() {
    let mut display = SimulatorDisplay::<BinaryColor>::new(Size::new(WIDTH, HEIGHT));
    display.clear(WHITE).unwrap();
    let dashboard = test_dashboard();
    DashboardRenderer::new()
        .render(&mut display, &dashboard.status, FIXTURE_TIME)
        .unwrap();

    let output_settings = OutputSettingsBuilder::new()
        .theme(BinaryColorTheme::OledWhite)
        .scale(2)
        .pixel_spacing(0)
        .build();

    println!("energydisplay simulator: showing {WIDTH}x{HEIGHT} advanced dashboard");
    println!("close the window or press Escape to exit");
    Window::new("Energy Display Simulator", &output_settings).show_static(&display);
}

#[cfg(test)]
mod tests {
    use super::*;
    use dashboard_core::renderer::BLACK;

    #[test]
    fn renders_advanced_dashboard_fixture() {
        let mut display = SimulatorDisplay::<BinaryColor>::new(Size::new(WIDTH, HEIGHT));
        display.clear(WHITE).unwrap();
        DashboardRenderer::new()
            .render(&mut display, &test_dashboard().status, FIXTURE_TIME)
            .unwrap();

        assert_eq!(display.get_pixel(Point::new(1, 1)), BLACK);
        assert_eq!(display.get_pixel(Point::new(199, 50)), WHITE);
        assert_eq!(display.get_pixel(Point::new(100, 147)), BLACK);
        assert_eq!(display.get_pixel(Point::new(201, 170)), BLACK);
    }

    #[test]
    fn renders_split_solar_variant() {
        let mut display = SimulatorDisplay::<BinaryColor>::new(Size::new(WIDTH, HEIGHT));
        display.clear(WHITE).unwrap();
        DashboardRenderer::new()
            .with_split_solar_production(true)
            .render(&mut display, &test_dashboard().status, FIXTURE_TIME)
            .unwrap();
    }
}
