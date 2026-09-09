use embedded_graphics::{pixelcolor::BinaryColor, prelude::*};
use embedded_graphics_simulator::{
    BinaryColorTheme, OutputSettingsBuilder, SimulatorDisplay, Window,
};
use energydisplay_firmware::{
    board::{DISPLAY_HEIGHT, DISPLAY_WIDTH},
    display::{WHITE, draw_hardware_check},
};

fn main() {
    let mut display =
        SimulatorDisplay::<BinaryColor>::new(Size::new(DISPLAY_WIDTH, DISPLAY_HEIGHT));
    display.clear(WHITE).unwrap();
    draw_hardware_check(&mut display).unwrap();

    let output_settings = OutputSettingsBuilder::new()
        .theme(BinaryColorTheme::OledWhite)
        .scale(2)
        .pixel_spacing(0)
        .build();

    println!("energydisplay simulator: showing {DISPLAY_WIDTH}x{DISPLAY_HEIGHT} output");
    println!("close the window or press Escape to exit");
    Window::new("Energy Display Simulator", &output_settings).show_static(&display);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_panel_frame() {
        let mut display =
            SimulatorDisplay::<BinaryColor>::new(Size::new(DISPLAY_WIDTH, DISPLAY_HEIGHT));
        display.clear(WHITE).unwrap();
        draw_hardware_check(&mut display).unwrap();

        assert_eq!(display.get_pixel(Point::new(200, 50)), WHITE);
        assert_eq!(display.get_pixel(Point::new(1, 1)), BinaryColor::Off);
        assert!((100..200).any(|y| {
            (100..300).any(|x| display.get_pixel(Point::new(x, y)) == BinaryColor::Off)
        }));
    }
}
