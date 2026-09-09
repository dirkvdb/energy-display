use embassy_executor::task;
use embassy_net::{Runner, Stack};
use embassy_time::{Duration, Timer};
use esp_println::println;
use esp_radio::wifi::{Interface, WifiController};

const INITIAL_RECONNECT_DELAY_SECS: u64 = 2;
const MAX_RECONNECT_DELAY_SECS: u64 = 30;

#[task]
pub async fn runner_task(mut runner: Runner<'static, Interface>) {
    runner.run().await
}

#[task]
pub async fn connection_task(mut controller: WifiController<'static>) {
    let mut reconnect_delay_secs = INITIAL_RECONNECT_DELAY_SECS;

    loop {
        println!("wifi: connecting");
        match controller.connect_async().await {
            Ok(_) => {
                println!("wifi: connected");
                reconnect_delay_secs = INITIAL_RECONNECT_DELAY_SECS;

                match controller.wait_for_disconnect_async().await {
                    Ok(_) => println!("wifi: disconnected"),
                    Err(error) => println!("wifi: disconnect wait failed: {:?}", error),
                }
            }
            Err(error) => println!("wifi: connection failed: {:?}", error),
        }

        println!("wifi: retrying in {}s", reconnect_delay_secs);
        Timer::after(Duration::from_secs(reconnect_delay_secs)).await;
        reconnect_delay_secs = (reconnect_delay_secs * 2).min(MAX_RECONNECT_DELAY_SECS);
    }
}

#[task]
pub async fn status_task(stack: Stack<'static>) {
    loop {
        stack.wait_config_up().await;
        if let Some(config) = stack.config_v4() {
            println!("network: DHCP address {}", config.address);
        } else {
            println!("network: configuration is up without IPv4");
        }

        stack.wait_config_down().await;
        println!("network: IPv4 configuration lost");
    }
}
