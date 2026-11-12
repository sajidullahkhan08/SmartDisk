//! Bounded-parallel filesystem scanner.

pub mod control;
pub mod engine;
pub mod progress;

pub use control::ScanControl;
pub use engine::{run_scan, ScanOptions};
pub use progress::{ProgressSnapshot, ScanProgress};
