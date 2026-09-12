use embassy_futures::join::join;
use embassy_time::{Duration, Timer};
use esp_hal::gpio::Input;
use log::info;

const DEBOUNCE_INTERVAL: Duration = Duration::from_millis(20);

/// Waits for active-low button interrupts and logs debounced presses.
#[embassy_executor::task]
pub async fn task(mut boot: Input<'static>, mut key: Input<'static>) -> ! {
    info!("buttons: monitoring BOOT GPIO0 (pressed=low)");
    info!("buttons: monitoring KEY GPIO18 (pressed=low)");
    join(monitor(&mut boot, "BOOT"), monitor(&mut key, "KEY")).await;
    unreachable!()
}

async fn monitor(button: &mut Input<'_>, name: &str) -> ! {
    loop {
        button.wait_for_falling_edge().await;
        Timer::after(DEBOUNCE_INTERVAL).await;

        if button.is_low() {
            info!("button: {} pressed", name);
            button.wait_for_high().await;
            Timer::after(DEBOUNCE_INTERVAL).await;
        }
    }
}
