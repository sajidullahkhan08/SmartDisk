//! SmartDisk core library.
//!
//! The crate is split into a pure-Rust analysis core (scanner, analysis,
//! intelligence, cleanup, storage) and a Slint presentation layer (`ui`, `app`).
//! Keeping the core independent from the UI makes it testable headlessly.

pub mod analysis;
pub mod app;
pub mod cleanup;
pub mod core;
pub mod file_operations;
pub mod intelligence;
pub mod platform;
pub mod scanner;
pub mod services;
pub mod storage;
pub mod ui;

pub use crate::core::models::{Category, Confidence, NodeId, RiskLevel};
