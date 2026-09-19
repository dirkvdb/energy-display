use core::fmt::Write as _;

use embassy_net::{
    Stack,
    dns::DnsSocket,
    tcp::client::{TcpClient, TcpClientState},
};
use embassy_time::{Duration, Timer, with_timeout};
use reqwless::{
    client::HttpClient,
    request::{Method, RequestBuilder},
};

use crate::{logging, panic_store};

const VICTORIA_LOGS_PORT: u16 = 9428;
const VICTORIA_PATH: &str = "/insert/jsonline?_stream_fields=app_name,hostname,proc_id";
const VICTORIA_HEADERS: [(&str, &str); 1] = [("Content-Type", "application/stream+json")];
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const SOCKET_TIMEOUT: Duration = Duration::from_secs(10);
const HTTP_HEADER_CAPACITY: usize = 512;
const TCP_TX_SIZE: usize = 256;
const TCP_RX_SIZE: usize = 1024;
const VICTORIA_BODY_CAPACITY: usize = 1600;

pub struct Buffers {
    tcp_state: TcpClientState<1, TCP_TX_SIZE, TCP_RX_SIZE>,
    victoria_url: heapless::String<96>,
    header: [u8; HTTP_HEADER_CAPACITY],
    body: heapless::String<VICTORIA_BODY_CAPACITY>,
    message: logging::StructuredLogMessage,
}

impl Buffers {
    pub const fn new() -> Self {
        Self {
            tcp_state: TcpClientState::new(),
            victoria_url: heapless::String::new(),
            header: [0; HTTP_HEADER_CAPACITY],
            body: heapless::String::new(),
            message: logging::StructuredLogMessage::empty(),
        }
    }

    pub fn initialize(&mut self) {
        write!(
            self.victoria_url,
            "http://logs.lan:{}{VICTORIA_PATH}",
            VICTORIA_LOGS_PORT
        )
        .expect("Victoria Logs URL exceeds capacity");
    }
}

impl Default for Buffers {
    fn default() -> Self {
        Self::new()
    }
}

/// Forwards warning and error records to Victoria Logs as JSON Lines.
#[embassy_executor::task]
pub async fn task(stack: Stack<'static>, buffers: &'static mut Buffers) -> ! {
    let dns = DnsSocket::new(stack);
    let mut failed = false;

    loop {
        let message = logging::next_structured_log_message().await;
        buffers.message = message;

        loop {
            stack.wait_config_up().await;
            buffers.body.clear();
            writeln!(buffers.body, "{}", buffers.message)
                .expect("Victoria Logs body exceeds capacity");

            let result: Result<(), ()> = async {
                let mut tcp_client = TcpClient::new(stack, &buffers.tcp_state);
                tcp_client.set_timeout(Some(SOCKET_TIMEOUT));
                let mut client = HttpClient::new(&tcp_client, &dns);
                let request = with_timeout(
                    CONNECT_TIMEOUT,
                    client.request(Method::POST, &buffers.victoria_url),
                )
                .await
                .map_err(|_| ())?
                .map_err(|_| ())?;
                let mut request = request
                    .headers(&VICTORIA_HEADERS)
                    .body(buffers.body.as_bytes());
                let response = request.send(&mut buffers.header).await.map_err(|_| ())?;
                (response.status.0 < 300).then_some(()).ok_or(())
            }
            .await;

            match result {
                Ok(()) => {
                    if failed {
                        logging::serial_only(
                            log::Level::Info,
                            format_args!("HTTP log forwarding restored"),
                        );
                        failed = false;
                    }
                    if buffers.message.is_persisted_panic()
                        && let Err(error) = panic_store::clear()
                    {
                        logging::serial_only(
                            log::Level::Error,
                            format_args!("failed to clear persisted panic: {:?}", error),
                        );
                    }
                    break;
                }
                Err(()) => {
                    if !failed {
                        logging::serial_only(
                            log::Level::Error,
                            format_args!("HTTP log forwarding failed"),
                        );
                        failed = true;
                    }
                    Timer::after_secs(1).await;
                    if !buffers.message.is_persisted_panic() {
                        break;
                    }
                }
            }
        }
    }
}
