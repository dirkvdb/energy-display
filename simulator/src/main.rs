use std::time::{SystemTime, UNIX_EPOCH};

use dashboard_core::{
    Dashboard, DashboardRenderer, LocalDateTime,
    renderer::{HEIGHT, WHITE, WIDTH},
};
use embassy_executor::{Executor, Spawner};
use embassy_net::{Config, Ipv4Address, Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4};
use embassy_net_tuntap::TunTapDevice;
use embassy_time::{Duration, Instant, Timer, with_timeout};
use embedded_graphics::{pixelcolor::BinaryColor, prelude::*};
use embedded_graphics_simulator::{
    BinaryColorTheme, OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window,
    sdl2::Keycode,
};
use energydisplay_firmware::tasks::mqtt;
use static_cell::StaticCell;

#[cfg(test)]
const FIXTURE_TIME: LocalDateTime = LocalDateTime::new(2026, 9, 9, 3, 12, 34);
const TAP_NAME: &str = "tap-energy";
const UI_POLL_INTERVAL: Duration = Duration::from_millis(16);

static EXECUTOR: StaticCell<Executor> = StaticCell::new();
static NETWORK_RESOURCES: StaticCell<StackResources<2>> = StaticCell::new();
static MQTT_BUFFERS: StaticCell<mqtt::Buffers> = StaticCell::new();

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(application_task(spawner).unwrap());
    });
}

#[embassy_executor::task]
async fn network_task(mut runner: Runner<'static, TunTapDevice>) {
    runner.run().await
}

#[embassy_executor::task]
async fn mqtt_task(stack: Stack<'static>, buffers: &'static mut mqtt::Buffers) {
    mqtt::run(stack, buffers).await;
}

#[embassy_executor::task]
async fn application_task(spawner: Spawner) {
    log::info!("energydisplay simulator starting on {TAP_NAME}");

    let device = TunTapDevice::new(TAP_NAME).unwrap_or_else(|error| {
        panic!("failed to open {TAP_NAME}; run `just simulator-tap-up`: {error}")
    });
    let config = Config::ipv4_static(StaticConfigV4 {
        address: Ipv4Cidr::new(Ipv4Address::new(192, 168, 69, 2), 24),
        dns_servers: Default::default(),
        gateway: Some(Ipv4Address::new(192, 168, 69, 1)),
    });
    let (stack, runner) = embassy_net::new(
        device,
        config,
        NETWORK_RESOURCES.init(StackResources::new()),
        network_seed(),
    );

    spawner.spawn(network_task(runner).unwrap());
    spawner.spawn(mqtt_task(stack, MQTT_BUFFERS.init(mqtt::Buffers::new())).unwrap());

    let mut display = SimulatorDisplay::<BinaryColor>::new(Size::new(WIDTH, HEIGHT));
    let mut dashboard = Dashboard::default();
    let mut renderer = DashboardRenderer::new();
    render_dashboard(&mut display, &mut renderer, &dashboard, application_time());

    let output_settings = OutputSettingsBuilder::new()
        .theme(BinaryColorTheme::OledWhite)
        .scale(2)
        .pixel_spacing(0)
        .build();
    let mut window = Window::new("Energy Display Simulator", &output_settings);
    window.update(&display);

    log::info!("showing {WIDTH}x{HEIGHT} live dashboard; close the window or press Escape to exit");

    loop {
        if let Ok(update) = with_timeout(UI_POLL_INTERVAL, mqtt::UPDATES.receive()).await {
            let now = application_time();
            dashboard.apply(update, now);
            render_dashboard(&mut display, &mut renderer, &dashboard, now);
        }

        window.update(&display);
        if window.events().any(|event| {
            matches!(
                event,
                SimulatorEvent::Quit
                    | SimulatorEvent::KeyDown {
                        keycode: Keycode::Escape,
                        ..
                    }
            )
        }) {
            std::process::exit(0);
        }

        Timer::after(Duration::from_millis(1)).await;
    }
}

fn render_dashboard(
    display: &mut SimulatorDisplay<BinaryColor>,
    renderer: &mut DashboardRenderer,
    dashboard: &Dashboard,
    now: LocalDateTime,
) {
    display.clear(WHITE).unwrap();
    renderer.render(display, &dashboard.status, now).unwrap();
}

fn network_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
        ^ u64::from(std::process::id())
}

/// Temporary monotonic display time matching the firmware until RTC/SNTP is implemented.
fn application_time() -> LocalDateTime {
    const START_MINUTE_OF_DAY: u64 = 12 * 60 + 34;
    let minute_of_day = (START_MINUTE_OF_DAY + Instant::now().as_secs() / 60) % (24 * 60);
    LocalDateTime::new(
        2026,
        9,
        9,
        3,
        (minute_of_day / 60) as u8,
        (minute_of_day % 60) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use dashboard_core::{model::test_dashboard, renderer::BLACK};

    #[test]
    fn renders_default_split_solar_dashboard_fixture() {
        let mut display = SimulatorDisplay::<BinaryColor>::new(Size::new(WIDTH, HEIGHT));
        display.clear(WHITE).unwrap();
        DashboardRenderer::new()
            .render(&mut display, &test_dashboard().status, FIXTURE_TIME)
            .unwrap();

        assert_eq!(display.get_pixel(Point::new(1, 1)), BLACK);
        assert_eq!(display.get_pixel(Point::new(199, 50)), WHITE);
        assert_eq!(display.get_pixel(Point::new(100, 147)), BLACK);
        assert_eq!(display.get_pixel(Point::new(201, 170)), BLACK);

        assert!(pixel_count(&display, Point::new(2, 2), Size::new(72, 72), WHITE) > 20);
        assert!(pixel_count(&display, Point::new(202, 2), Size::new(70, 72), WHITE) > 20);
        assert!(pixel_count(&display, Point::new(115, 231), Size::new(30, 30), WHITE) > 20);
        assert!(pixel_count(&display, Point::new(294, 224), Size::new(24, 35), BLACK) > 20);
        assert!(pixel_count(&display, Point::new(294, 261), Size::new(24, 36), BLACK) > 20);
    }

    fn pixel_count(
        display: &SimulatorDisplay<BinaryColor>,
        top_left: Point,
        size: Size,
        color: BinaryColor,
    ) -> usize {
        (top_left.y..top_left.y + size.height as i32)
            .flat_map(|y| {
                (top_left.x..top_left.x + size.width as i32).map(move |x| Point::new(x, y))
            })
            .filter(|point| display.get_pixel(*point) == color)
            .count()
    }

    #[test]
    fn renders_combined_solar_variant() {
        let mut display = SimulatorDisplay::<BinaryColor>::new(Size::new(WIDTH, HEIGHT));
        display.clear(WHITE).unwrap();
        DashboardRenderer::new()
            .with_split_solar_production(false)
            .render(&mut display, &test_dashboard().status, FIXTURE_TIME)
            .unwrap();
    }
}
