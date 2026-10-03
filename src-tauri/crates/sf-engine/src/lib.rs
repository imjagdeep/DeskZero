//! Super Folder engine — pure Rust, no Tauri dependencies.
//!
//! Everything testable lives here. The Tauri command layer (later) is a thin
//! adapter over this crate.

pub mod classify;
pub mod config;
pub mod fsutil;
pub mod history;
pub mod mover;
pub mod plan;
pub mod rules;
pub mod types;
pub mod watcher;

pub use config::Config;
pub use history::{HistoryEntry, HistoryKind};
pub use plan::{file_meta, plan_inputs};
pub use types::*;
