#![no_std]
#![no_main]

use display_interface_spi::SPIInterface;
use embassy_executor::Spawner;
use embassy_time::Timer;
use embedded_hal_bus::spi::ExclusiveDevice;
use energydisplay_firmware::{board, display};
use esp_backtrace as _;
use esp_hal::{
    gpio::{Level, Output, OutputConfig},
    spi::{
        Mode,
        master::{Config as SpiConfig, Spi},
    },
    time::Rate,
    timer::timg::TimerGroup,
};
use esp_println::println;
use st7305::{Orientation, St7305};

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal::main]
async fn main(_spawner: Spawner) {
    println!("energydisplay board-support firmware starting");

    let peripherals = esp_hal::init(esp_hal::Config::default());
    let timer_group = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timer_group.timer0, peripherals.FROM_CPU_INTR0);

    println!(
        "ST7305 native={}x{}, logical={}x{} landscape",
        board::DISPLAY_NATIVE_WIDTH,
        board::DISPLAY_NATIVE_HEIGHT,
        board::DISPLAY_WIDTH,
        board::DISPLAY_HEIGHT,
    );
    println!(
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

    println!("initializing ST7305");
    let mut delay = embassy_time::Delay;
    panel.init_async(&mut delay).await.unwrap();
    panel.set_orientation(Orientation::Landscape);

    display::clear_white(&mut panel);
    display::draw_hardware_check(&mut panel).unwrap();
    panel.flush().unwrap();
    println!("display hardware check rendered");

    loop {
        Timer::after_secs(board::HEARTBEAT_INTERVAL_SECS).await;
        println!("heartbeat: display initialized");
    }
}
