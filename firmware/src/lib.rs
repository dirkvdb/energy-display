#![no_std]

#[cfg(target_arch = "xtensa")]
use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::null_mut,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[cfg(target_arch = "xtensa")]
struct BoundedPostBootAllocator;

// This gates Rust's `GlobalAlloc` path only. The radio's C compatibility shims call
// `esp_alloc::HEAP` directly, so bounded vendor packet allocations remain operational.
#[cfg(target_arch = "xtensa")]
#[global_allocator]
static GLOBAL_ALLOCATOR: BoundedPostBootAllocator = BoundedPostBootAllocator;
#[cfg(target_arch = "xtensa")]
static RUST_ALLOCATIONS_ALLOWED: AtomicBool = AtomicBool::new(true);
#[cfg(target_arch = "xtensa")]
static REJECTED_ALLOCATION_SIZE: AtomicUsize = AtomicUsize::new(0);

/// Maximum individual Rust allocation permitted after boot for radio/RTOS bookkeeping.
pub const MAX_POST_BOOT_RUST_ALLOCATION_SIZE: usize = 1024;

/// Restricts post-boot Rust heap allocations to small radio/RTOS bookkeeping objects.
#[cfg(target_arch = "xtensa")]
pub fn seal_allocator() {
    RUST_ALLOCATIONS_ALLOWED.store(false, Ordering::Release);
}

#[cfg(not(target_arch = "xtensa"))]
pub const fn seal_allocator() {}

#[cfg(target_arch = "xtensa")]
pub(crate) fn rejected_allocation_size() -> Option<usize> {
    match REJECTED_ALLOCATION_SIZE.load(Ordering::Acquire) {
        0 => None,
        size => Some(size),
    }
}

#[cfg(target_arch = "xtensa")]
fn reject_allocation(layout: Layout) -> *mut u8 {
    let _ = REJECTED_ALLOCATION_SIZE.compare_exchange(
        0,
        layout.size().max(1),
        Ordering::AcqRel,
        Ordering::Acquire,
    );
    null_mut()
}

#[cfg(target_arch = "xtensa")]
unsafe impl GlobalAlloc for BoundedPostBootAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if rust_allocation_allowed(layout.size()) {
            unsafe { GlobalAlloc::alloc(&esp_alloc::HEAP, layout) }
        } else {
            reject_allocation(layout)
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if rust_allocation_allowed(layout.size()) {
            unsafe { GlobalAlloc::alloc_zeroed(&esp_alloc::HEAP, layout) }
        } else {
            reject_allocation(layout)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { GlobalAlloc::dealloc(&esp_alloc::HEAP, ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if rust_allocation_allowed(new_size) {
            unsafe { GlobalAlloc::realloc(&esp_alloc::HEAP, ptr, layout, new_size) }
        } else {
            reject_allocation(Layout::from_size_align(new_size, layout.align()).unwrap_or(layout))
        }
    }
}

#[cfg(target_arch = "xtensa")]
fn rust_allocation_allowed(size: usize) -> bool {
    RUST_ALLOCATIONS_ALLOWED.load(Ordering::Acquire) || size <= MAX_POST_BOOT_RUST_ALLOCATION_SIZE
}

pub mod board;
pub mod clock;
pub mod config;
pub mod display;
pub mod logging;
pub mod panic_store;
pub mod tasks;
