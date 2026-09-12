#[cfg(any(target_arch = "xtensa", test))]
mod record {
    pub const PANIC_MESSAGE_CAPACITY: usize = 512;

    pub type PanicMessage = heapless::String<PANIC_MESSAGE_CAPACITY>;

    const RECORD_MAGIC: &[u8; 8] = b"EDPANIC!";
    const RECORD_VERSION: u8 = 1;
    pub(super) const RECORD_HEADER_SIZE: usize = 16;
    const RECORD_SIZE: usize = RECORD_HEADER_SIZE + PANIC_MESSAGE_CAPACITY;

    #[repr(C, align(4))]
    pub(super) struct AlignedRecord(pub(super) [u8; RECORD_SIZE]);

    impl AlignedRecord {
        pub(super) fn erased() -> Self {
            Self([0xff; RECORD_SIZE])
        }
    }

    pub(super) fn encode(message: &str) -> AlignedRecord {
        let bytes = message.as_bytes();
        let length = bytes.len().min(PANIC_MESSAGE_CAPACITY);
        let mut record = AlignedRecord::erased();
        record.0[..8].copy_from_slice(RECORD_MAGIC);
        record.0[8] = RECORD_VERSION;
        record.0[9] = 0;
        record.0[10..12].copy_from_slice(&(length as u16).to_le_bytes());
        record.0[RECORD_HEADER_SIZE..RECORD_HEADER_SIZE + length].copy_from_slice(&bytes[..length]);
        let checksum = checksum(&record.0[8..12], &record.0[RECORD_HEADER_SIZE..][..length]);
        record.0[12..16].copy_from_slice(&checksum.to_le_bytes());
        record
    }

    pub(super) fn decode(record: &AlignedRecord) -> Option<PanicMessage> {
        if &record.0[..8] != RECORD_MAGIC || record.0[8] != RECORD_VERSION {
            return None;
        }

        let length = u16::from_le_bytes([record.0[10], record.0[11]]) as usize;
        if length > PANIC_MESSAGE_CAPACITY {
            return None;
        }

        let expected = u32::from_le_bytes(record.0[12..16].try_into().ok()?);
        let payload = &record.0[RECORD_HEADER_SIZE..][..length];
        if checksum(&record.0[8..12], payload) != expected {
            return None;
        }

        let payload = core::str::from_utf8(payload).ok()?;
        let mut message = PanicMessage::new();
        message.push_str(payload).ok()?;
        Some(message)
    }

    fn checksum(header: &[u8], payload: &[u8]) -> u32 {
        let mut value = 0x811c_9dc5u32;
        for byte in header.iter().chain(payload) {
            value ^= u32::from(*byte);
            value = value.wrapping_mul(0x0100_0193);
        }
        value
    }
}

#[cfg(target_arch = "xtensa")]
mod hardware {
    use core::{
        cell::UnsafeCell,
        fmt::Write as _,
        panic::PanicInfo,
        sync::atomic::{AtomicBool, Ordering},
    };

    use esp_bootloader_esp_idf::partitions::{self, PARTITION_TABLE_MAX_LEN};
    use esp_storage::{Flash, FlashStorage, FlashStorageError};

    use super::record::{AlignedRecord, PanicMessage, decode, encode};

    const PARTITION_LABEL: &str = "panic";

    #[derive(Debug)]
    pub enum InitializeError {
        PartitionTable(partitions::Error),
        MissingPartition,
        InvalidPartition,
        Flash(FlashStorageError),
        AlreadyInitialized,
    }

    #[derive(Debug)]
    pub enum StorageError {
        Unavailable,
        Busy,
        Flash(FlashStorageError),
    }

    struct PanicFlash {
        storage: FlashStorage<'static>,
        offset: u32,
    }

    struct PanicFlashCell(UnsafeCell<Option<PanicFlash>>);

    // Access is serialized by `FLASH_BUSY`. Initialization happens before the
    // executor starts, and the panic path never waits for an interrupted user.
    unsafe impl Sync for PanicFlashCell {}

    static PANIC_FLASH: PanicFlashCell = PanicFlashCell(UnsafeCell::new(None));
    static FLASH_BUSY: AtomicBool = AtomicBool::new(false);
    static PANICKING: AtomicBool = AtomicBool::new(false);

    #[repr(C, align(4))]
    struct PartitionTableBuffer([u8; PARTITION_TABLE_MAX_LEN]);

    struct FlashGuard;

    impl Drop for FlashGuard {
        fn drop(&mut self) {
            FLASH_BUSY.store(false, Ordering::Release);
        }
    }

    pub fn initialize(flash: Flash<'static>) -> Result<Option<PanicMessage>, InitializeError> {
        let mut storage = FlashStorage::new(flash).multicore_auto_park();
        let mut table_buffer = PartitionTableBuffer([0; PARTITION_TABLE_MAX_LEN]);
        let table = partitions::read_partition_table(&mut storage, &mut table_buffer.0)
            .map_err(InitializeError::PartitionTable)?;
        let partition = table
            .iter()
            .find(|entry| entry.label_as_str() == PARTITION_LABEL)
            .ok_or(InitializeError::MissingPartition)?;

        if partition.offset() % FlashStorage::SECTOR_SIZE != 0
            || partition.len() < FlashStorage::SECTOR_SIZE
            || partition.is_read_only()
        {
            return Err(InitializeError::InvalidPartition);
        }

        install(PanicFlash {
            storage,
            offset: partition.offset(),
        })?;
        read_pending().map_err(|error| match error {
            StorageError::Flash(error) => InitializeError::Flash(error),
            StorageError::Unavailable | StorageError::Busy => InitializeError::AlreadyInitialized,
        })
    }

    pub fn clear() -> Result<(), StorageError> {
        with_flash(|flash| {
            flash
                .storage
                .erase(flash.offset, flash.offset + FlashStorage::SECTOR_SIZE)
        })
    }

    fn install(flash: PanicFlash) -> Result<(), InitializeError> {
        let _guard = acquire_flash().map_err(|_| InitializeError::AlreadyInitialized)?;
        // SAFETY: `FLASH_BUSY` grants exclusive access to the cell.
        let slot = unsafe { &mut *PANIC_FLASH.0.get() };
        if slot.is_some() {
            return Err(InitializeError::AlreadyInitialized);
        }
        *slot = Some(flash);
        Ok(())
    }

    fn read_pending() -> Result<Option<PanicMessage>, StorageError> {
        with_flash(|flash| {
            let mut record = AlignedRecord::erased();
            flash.storage.read_nor(flash.offset, &mut record.0)?;
            Ok(decode(&record))
        })
    }

    fn persist(info: &PanicInfo<'_>) -> Result<(), StorageError> {
        let mut message = PanicMessage::new();
        let _ = write!(message, "{info}");
        let record = encode(&message);
        with_flash(|flash| {
            flash
                .storage
                .erase(flash.offset, flash.offset + FlashStorage::SECTOR_SIZE)?;
            flash.storage.write_nor(flash.offset, &record.0)
        })
    }

    fn acquire_flash() -> Result<FlashGuard, StorageError> {
        FLASH_BUSY
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map(|_| FlashGuard)
            .map_err(|_| StorageError::Busy)
    }

    fn with_flash<R>(
        operation: impl FnOnce(&mut PanicFlash) -> Result<R, FlashStorageError>,
    ) -> Result<R, StorageError> {
        let _guard = acquire_flash()?;
        // SAFETY: `FLASH_BUSY` grants exclusive mutable access to the cell.
        let flash = unsafe { &mut *PANIC_FLASH.0.get() }
            .as_mut()
            .ok_or(StorageError::Unavailable)?;
        operation(flash).map_err(StorageError::Flash)
    }

    #[panic_handler]
    fn panic_handler(info: &PanicInfo<'_>) -> ! {
        if PANICKING.swap(true, Ordering::Relaxed) {
            esp_println::println!("recursive panic; resetting");
            esp_hal::system::software_reset();
        }

        esp_println::println!("");
        esp_println::println!("====================== PANIC ======================");
        esp_println::println!("{info}");
        if let Err(error) = persist(info) {
            esp_println::println!("failed to persist panic: {:?}", error);
        }

        esp_println::println!("");
        esp_println::println!("Backtrace:");
        for frame in esp_backtrace::Backtrace::capture().frames() {
            esp_println::println!("0x{:x}", frame.program_counter());
        }
        esp_println::println!("resetting");
        esp_hal::system::software_reset()
    }
}

#[cfg(target_arch = "xtensa")]
pub use hardware::{InitializeError, StorageError, clear, initialize};
#[cfg(target_arch = "xtensa")]
pub use record::{PANIC_MESSAGE_CAPACITY, PanicMessage};

#[cfg(test)]
mod tests {
    use super::record::*;

    #[test]
    fn record_round_trips() {
        let record = encode("panicked at firmware/src/main.rs:42: display failed");
        assert_eq!(
            decode(&record).as_deref(),
            Some("panicked at firmware/src/main.rs:42: display failed")
        );
    }

    #[test]
    fn erased_and_corrupt_records_are_ignored() {
        assert!(decode(&AlignedRecord::erased()).is_none());

        let mut record = encode("panic");
        record.0[RECORD_HEADER_SIZE] ^= 1;
        assert!(decode(&record).is_none());
    }
}
