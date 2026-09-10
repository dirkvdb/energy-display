use core::net::Ipv4Addr;

pub const WIFI_SSID: &str = env!("WIFI_SSID");
pub const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");

pub const NTP_SERVER_ADDRESS: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 1);

pub const HOME_SERVER_ADDRESS: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 13);
pub const MQTT_BROKER_ADDRESS: Ipv4Addr = HOME_SERVER_ADDRESS;
pub const MQTT_PORT: u16 = 1883;
#[cfg(not(feature = "sim"))]
pub const MQTT_CLIENT_ID: &str = "energydisplay";
#[cfg(feature = "sim")]
pub const MQTT_CLIENT_ID: &str = "energydisplay-simulator";
pub const MQTT_USERNAME: &str = "iot";
pub const MQTT_PASSWORD: &str = env!("MQTT_PASSWORD");
pub const MQTT_KEEP_ALIVE_SECONDS: u16 = 180;
