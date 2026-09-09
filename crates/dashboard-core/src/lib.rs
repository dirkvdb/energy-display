#![no_std]

extern crate alloc;

mod font;
pub mod formatting;
pub mod model;
pub mod renderer;
pub mod routing;

pub use model::{Dashboard, EnergyStatus, LocalDateTime};
pub use renderer::{BLACK, DashboardRenderer, WHITE};
