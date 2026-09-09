#![no_std]

pub mod board;
#[cfg(target_arch = "xtensa")]
pub mod config;
pub mod display;
#[cfg(target_arch = "xtensa")]
pub mod tasks;
