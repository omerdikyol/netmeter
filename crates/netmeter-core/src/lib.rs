//! Core sampling, storage and reporting engine for NetMeter.
//!
//! This crate is deliberately UI-free so the behaviour can be unit-tested and
//! reused by the tray app, the CLI, or anything else.

pub mod config;
pub mod format;
pub mod model;
pub mod sampler;
pub mod stats;
pub mod store;
pub mod tracker;

pub use config::{Appearance, Config, Cycle, MenuBarMode, Plan, Theme, UnitSystem};
pub use model::{Rate, Traffic};
pub use stats::Range;
pub use tracker::Tracker;
