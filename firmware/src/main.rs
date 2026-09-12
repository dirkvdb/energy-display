#![no_std]
#![no_main]

use core::mem::MaybeUninit;

use dashboard_core::{Dashboard, DashboardRenderer};
use display_interface_spi::SPIInterface;
use embassy_executor::Spawner;
use embassy_futures::{
    join::{join, join5},
    select::{Either, select},
};
use embassy_net::StackResources;
use embassy_time::{Duration, Ticker};
use embedded_hal_bus::spi::ExclusiveDevice;
use energydisplay_firmware::{
    board, clock, config, display, logging,
    tasks::{mqtt, net, structured_log},
};
use esp_backtrace as _;
use esp_hal::{
    gpio::{Level, Output, OutputConfig},
    ram,
    rng::Rng,
    spi::{
        Mode,
        master::{Config as SpiConfig, Spi},
    },
    time::Rate,
    timer::timg::TimerGroup,
};
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config as WifiConfig, ControllerConfig, Interface, WifiController,
    sta::StationConfig,
};
use log::info;
use st7305::{Orientation, St7305};
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

// MQTT, SNTP, and structured-log HTTP can all run alongside DHCP. Keep the
// socket set in stable static storage outside the protected main-stack region.
#[ram(reclaimed)]
static mut NETWORK_RESOURCES: MaybeUninit<StackResources<4>> = MaybeUninit::uninit();
static MQTT_BUFFERS: StaticCell<mqtt::Buffers> = StaticCell::new();

#[esp_hal::main]
async fn main(spawner: Spawner) -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let previous_panic = panic_store::initialize(peripherals.FLASH);
    logging::initialize();
    match previous_panic {
        Ok(Some(message)) => logging::report_previous_panic(&message),
        Ok(None) => {}
        Err(error) => log::error!("panic storage initialization failed: {:?}", error),
    }
    info!("energydisplay firmware starting");
    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: board::RADIO_HEAP_SIZE);
    esp_alloc::heap_allocator!(size: board::FONT_HEAP_SIZE);

    let timer_group = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timer_group.timer0, peripherals.FROM_CPU_INTR0);

    info!(
        "ST7305 native={}x{}, logical={}x{} landscape",
        board::DISPLAY_NATIVE_WIDTH,
        board::DISPLAY_NATIVE_HEIGHT,
        board::DISPLAY_WIDTH,
        board::DISPLAY_HEIGHT,
    );
    info!(
        "SPI mode {} at {}MHz: SCLK=GPIO{}, MOSI=GPIO{}, DC=GPIO{}, CS=GPIO{}, RESET=GPIO{}; TE=GPIO{} unused",
        board::DISPLAY_SPI_MODE,
        board::DISPLAY_SPI_FREQUENCY_MHZ,
        board::DISPLAY_SCLK_GPIO,
        board::DISPLAY_MOSI_GPIO,
        board::DISPLAY_DC_GPIO,
        board::DISPLAY_CS_GPIO,
        board::DISPLAY_RESET_GPIO,
        board::DISPLAY_TE_GPIO,
    );

    let spi_bus = Spi::new(
        peripherals.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_mhz(board::DISPLAY_SPI_FREQUENCY_MHZ))
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(peripherals.GPIO11)
    .with_mosi(peripherals.GPIO12);

    let chip_select = Output::new(peripherals.GPIO40, Level::High, OutputConfig::default());
    let data_command = Output::new(peripherals.GPIO5, Level::Low, OutputConfig::default());
    let reset = Output::new(peripherals.GPIO41, Level::High, OutputConfig::default());

    let spi_device = ExclusiveDevice::new(spi_bus, chip_select, embassy_time::Delay).unwrap();
    let interface = SPIInterface::new(spi_device, data_command);
    let mut panel = St7305::new(interface, reset);

    info!("display: initializing ST7305");
    let mut delay = embassy_time::Delay;
    panel.init_async(&mut delay).await.unwrap();
    panel.set_orientation(Orientation::Landscape);
    info!("display: ST7305 initialized");

    let mut dashboard = Dashboard::default();
    info!("display: initializing dashboard renderer");
    let mut renderer = DashboardRenderer::new();
    info!("display: dashboard renderer initialized");

    display::clear_white(&mut panel);
    info!("display: framebuffer cleared; rendering empty dashboard");
    renderer
        .render(&mut panel, &dashboard.status, clock::now())
        .unwrap();
    info!("display: empty dashboard rasterized; flushing panel");
    panel.flush().unwrap();
    info!("display: empty dashboard rendered");
    let heap_stats = esp_alloc::HEAP.stats();
    info!(
        "heap: {} of {} bytes used after display initialization",
        heap_stats.current_usage, heap_stats.size
    );

    let station_config = WifiConfig::Station(
        StationConfig::default()
            .with_ssid(
                config::WIFI_SSID
                    .try_into()
                    .expect("WIFI_SSID exceeds the Wi-Fi driver limit"),
            )
            .with_authentication(AuthenticationMethodConfig::Wpa2Personal(
                config::WIFI_PASSWORD
                    .try_into()
                    .expect("WIFI_PASSWORD exceeds the Wi-Fi driver limit"),
            )),
    );

    info!("wifi: initializing station");
    let wifi_interface = Interface::station();
    let controller = WifiController::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(station_config),
    )
    .expect("failed to initialize Wi-Fi controller");

    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;
    // SAFETY: `main` initializes this uninitialized static exactly once and
    // Embassy owns the returned reference for the rest of the program.
    let network_resources = unsafe {
        let storage = core::ptr::addr_of_mut!(NETWORK_RESOURCES);
        (*storage).write(StackResources::new())
    };
    let (stack, runner) = embassy_net::new(
        wifi_interface,
        embassy_net::Config::dhcpv4(Default::default()),
        network_resources,
        seed,
    );
    let heap_stats = esp_alloc::HEAP.stats();
    info!(
        "heap: {} of {} bytes used after display and network initialization",
        heap_stats.current_usage, heap_stats.size
    );

    spawner.spawn(structured_log::task(stack).unwrap());

    // Borrow the large panel/dashboard state instead of moving it into a second
    // future, which creates large temporaries in the main task's poll frame.
    let display_task = async {
        let mut clock_tick = Ticker::every(Duration::from_secs(1));
        let mut displayed_time = None;
        loop {
            let event = select(mqtt::UPDATES.receive(), clock_tick.next()).await;
            let now = clock::now();
            match event {
                Either::First(update) => dashboard.apply(update, now),
                Either::Second(_) if now == displayed_time => continue,
                Either::Second(_) => {}
            }
            display::clear_white(&mut panel);
            renderer.render(&mut panel, &dashboard.status, now).unwrap();
            panel.flush().unwrap();
            displayed_time = now;
        }
    };

    join5(
        net::runner_task(runner),
        net::connection_task(controller),
        join(net::status_task(stack), clock::run(stack)),
        mqtt::run(stack, MQTT_BUFFERS.init(mqtt::Buffers::new())),
        display_task,
    )
    .await;

    panic!("a long-lived firmware task exited unexpectedly")
}
