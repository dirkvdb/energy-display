use core::cmp::min;

use embassy_net::{Stack, tcp::TcpSocket};
use embassy_time::{Duration, Timer};
use esp_bootloader_esp_idf::{
    ota::OtaImageState,
    ota_updater::OtaUpdater,
    partitions::{self, AppPartitionSubType, Error as PartitionError, PartitionType},
};
use esp_storage::{FlashStorage, FlashStorageError};
use hmac::{Hmac, KeyInit, Mac};
use log::{error, info, warn};
use sha2::{Digest, Sha256};

use crate::{config, panic_store};

const MAGIC: &[u8; 8] = b"EDOTA001";
const HEADER_SIZE: usize = 8 + 4 + 32 + 32;
const DIGEST_OFFSET: usize = 12;
const MAC_OFFSET: usize = 44;
const FLASH_CHUNK_SIZE: usize = 2048;
const TCP_RX_SIZE: usize = 2048;
const TCP_TX_SIZE: usize = 128;
const SOCKET_TIMEOUT: Duration = Duration::from_secs(120);

type HmacSha256 = Hmac<Sha256>;

#[repr(C, align(4))]
struct FlashChunk([u8; FLASH_CHUNK_SIZE]);

pub struct Buffers {
    tcp_rx: [u8; TCP_RX_SIZE],
    tcp_tx: [u8; TCP_TX_SIZE],
    flash: FlashChunk,
}

impl Buffers {
    pub const fn new() -> Self {
        Self {
            tcp_rx: [0; TCP_RX_SIZE],
            tcp_tx: [0; TCP_TX_SIZE],
            flash: FlashChunk([0; FLASH_CHUNK_SIZE]),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug)]
enum UpdateError {
    Network(embassy_net::tcp::Error),
    TransferNetwork {
        written: u32,
        source: embassy_net::tcp::Error,
    },
    Closed,
    InvalidHeader,
    Authentication,
    InvalidSize,
    InvalidImage,
    DigestMismatch,
    FlashBusy,
    FlashUnavailable,
    Flash(FlashStorageError),
    Partition(PartitionError),
}

struct Target {
    offset: u32,
    size: u32,
    subtype: AppPartitionSubType,
}

pub fn confirm_running_image() {
    let result = with_flash(|storage| {
        let mut table_buffer = [0; partitions::PARTITION_TABLE_MAX_LEN];
        let mut updater = OtaUpdater::new(storage, &mut table_buffer)?;
        match updater.current_ota_state() {
            Ok(OtaImageState::New | OtaImageState::PendingVerify) => {
                updater.set_current_ota_state(OtaImageState::Valid)?;
                info!("ota: confirmed running image");
            }
            Ok(_) | Err(PartitionError::InvalidState) => {}
            Err(error) => return Err(error.into()),
        }
        Ok(())
    });

    if let Err(error) = result {
        warn!("ota: unable to confirm running image: {:?}", error);
    }
}

#[embassy_executor::task]
pub async fn task(stack: Stack<'static>, buffers: &'static mut Buffers) -> ! {
    loop {
        stack.wait_config_up().await;
        let mut socket = TcpSocket::new(stack, &mut buffers.tcp_rx, &mut buffers.tcp_tx);
        socket.set_timeout(Some(SOCKET_TIMEOUT));

        info!("ota: listening on TCP port {}", config::OTA_PORT);
        if let Err(error) = socket.accept(config::OTA_PORT).await {
            warn!("ota: accept failed: {:?}", error);
            continue;
        }

        info!("ota: upload connection accepted");
        match receive_update(&mut socket, &mut buffers.flash).await {
            Ok(()) => {
                let _ = write_all(&mut socket, b"OK rebooting\n").await;
                let _ = socket.flush().await;
                info!("ota: update installed; rebooting");
                Timer::after_millis(250).await;
                esp_hal::system::software_reset();
            }
            Err(error) => {
                error!("ota: update rejected: {:?}", error);
                let _ = write_all(&mut socket, b"ERR update rejected\n").await;
                socket.close();
            }
        }
    }
}

async fn receive_update(
    socket: &mut TcpSocket<'_>,
    flash_buffer: &mut FlashChunk,
) -> Result<(), UpdateError> {
    let mut header = [0; HEADER_SIZE];
    read_exact(socket, &mut header).await?;
    if &header[..MAGIC.len()] != MAGIC {
        return Err(UpdateError::InvalidHeader);
    }

    let image_size = u32::from_le_bytes(
        header[8..12]
            .try_into()
            .map_err(|_| UpdateError::InvalidHeader)?,
    );
    if image_size == 0 || !image_size.is_multiple_of(4) {
        return Err(UpdateError::InvalidSize);
    }

    let mut mac = HmacSha256::new_from_slice(config::OTA_PASSWORD.as_bytes())
        .map_err(|_| UpdateError::Authentication)?;
    mac.update(&header[..MAC_OFFSET]);
    mac.verify_slice(&header[MAC_OFFSET..])
        .map_err(|_| UpdateError::Authentication)?;

    let target = prepare_target(image_size)?;
    info!(
        "ota: receiving {} bytes into {:?}",
        image_size, target.subtype
    );
    write_all(socket, b"READY\n").await?;
    socket.flush().await.map_err(UpdateError::Network)?;

    let mut hasher = Sha256::new();
    let mut written = 0u32;
    while written < image_size {
        let chunk_len = min(FLASH_CHUNK_SIZE as u32, image_size - written) as usize;
        if let Err(error) = read_exact(socket, &mut flash_buffer.0[..chunk_len]).await {
            return Err(match error {
                UpdateError::Network(source) => UpdateError::TransferNetwork { written, source },
                error => error,
            });
        }
        if written == 0 && flash_buffer.0[0] != 0xe9 {
            return Err(UpdateError::InvalidImage);
        }
        Digest::update(&mut hasher, &flash_buffer.0[..chunk_len]);
        write_chunk(target.offset + written, &flash_buffer.0[..chunk_len])?;
        written += chunk_len as u32;
        if written % (256 * 1024) == 0 || written == image_size {
            info!("ota: received {} of {} bytes", written, image_size);
        }
    }

    let digest = hasher.finalize();
    if digest[..] != header[DIGEST_OFFSET..MAC_OFFSET] {
        return Err(UpdateError::DigestMismatch);
    }

    validate_and_activate(&target)?;
    Ok(())
}

fn prepare_target(image_size: u32) -> Result<Target, UpdateError> {
    with_flash(|storage| {
        if esp_storage::flash_encryption() {
            return Err(UpdateError::InvalidImage);
        }

        let mut table_buffer = [0; partitions::PARTITION_TABLE_MAX_LEN];
        let mut updater = OtaUpdater::new(storage, &mut table_buffer)?;
        if updater.selected_partition().is_err() {
            updater.reset_data()?;
        }
        let (mut region, subtype) = updater.next_partition()?;
        let size = region.capacity() as u32;
        if image_size > size {
            return Err(UpdateError::InvalidSize);
        }
        let erase_end = image_size.next_multiple_of(FlashStorage::SECTOR_SIZE);
        region.erase(0, erase_end)?;
        drop(region);
        drop(updater);

        let table = partitions::read_partition_table(storage, &mut table_buffer)?;
        let entry = table
            .iter()
            .find(|entry| entry.partition_type() == PartitionType::App(subtype))
            .ok_or(UpdateError::InvalidImage)?;
        Ok(Target {
            offset: entry.offset(),
            size,
            subtype,
        })
    })
}

fn write_chunk(offset: u32, chunk: &[u8]) -> Result<(), UpdateError> {
    with_flash(|storage| {
        storage.write_nor(offset, chunk)?;
        Ok(())
    })
}

fn validate_and_activate(target: &Target) -> Result<(), UpdateError> {
    with_flash(|storage| {
        let mut table_buffer = [0; partitions::PARTITION_TABLE_MAX_LEN];
        let table = partitions::read_partition_table(storage, &mut table_buffer)?;
        let entry = table
            .iter()
            .find(|entry| entry.partition_type() == PartitionType::App(target.subtype))
            .ok_or(UpdateError::InvalidImage)?;
        if entry.offset() != target.offset || entry.len() != target.size {
            return Err(UpdateError::InvalidImage);
        }
        entry.sha256(storage)?;

        let mut updater = OtaUpdater::new(storage, &mut table_buffer)?;
        let (_, subtype) = updater.next_partition()?;
        if subtype != target.subtype {
            return Err(UpdateError::InvalidImage);
        }
        updater.activate_next_partition()?;
        updater.set_current_ota_state(OtaImageState::New)?;
        Ok(())
    })
}

fn with_flash<R>(
    operation: impl FnOnce(&mut FlashStorage<'static>) -> Result<R, UpdateError>,
) -> Result<R, UpdateError> {
    panic_store::with_storage(operation).map_err(|error| match error {
        panic_store::FlashAccessError::Unavailable => UpdateError::FlashUnavailable,
        panic_store::FlashAccessError::Busy => UpdateError::FlashBusy,
        panic_store::FlashAccessError::Operation(error) => error,
    })
}

async fn read_exact(socket: &mut TcpSocket<'_>, mut buffer: &mut [u8]) -> Result<(), UpdateError> {
    while !buffer.is_empty() {
        let read = socket.read(buffer).await.map_err(UpdateError::Network)?;
        if read == 0 {
            return Err(UpdateError::Closed);
        }
        buffer = &mut buffer[read..];
    }
    Ok(())
}

async fn write_all(socket: &mut TcpSocket<'_>, mut bytes: &[u8]) -> Result<(), UpdateError> {
    while !bytes.is_empty() {
        let written = socket.write(bytes).await.map_err(UpdateError::Network)?;
        if written == 0 {
            return Err(UpdateError::Closed);
        }
        bytes = &bytes[written..];
    }
    Ok(())
}

impl From<PartitionError> for UpdateError {
    fn from(error: PartitionError) -> Self {
        Self::Partition(error)
    }
}

impl From<FlashStorageError> for UpdateError {
    fn from(error: FlashStorageError) -> Self {
        Self::Flash(error)
    }
}
