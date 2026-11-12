use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub use crate::core::models::ProgressSnapshot;

/// Lock-free counters plus a short description of current activity.
#[derive(Debug)]
pub struct ScanProgress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    pub errors: AtomicU64,
    pub skipped_reparse: AtomicU64,
    current_path: Mutex<String>,
    started: Instant,
}

impl Default for ScanProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl ScanProgress {
    pub fn new() -> Self {
        Self {
            files: AtomicU64::new(0),
            dirs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            skipped_reparse: AtomicU64::new(0),
            current_path: Mutex::new(String::new()),
            started: Instant::now(),
        }
    }

    pub fn set_current(&self, path: &str) {
        if let Ok(mut cur) = self.current_path.lock() {
            cur.clear();
            cur.push_str(path);
        }
    }

    pub fn snapshot(&self) -> ProgressSnapshot {
        ProgressSnapshot {
            files: self.files.load(Ordering::Relaxed),
            dirs: self.dirs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            skipped_reparse: self.skipped_reparse.load(Ordering::Relaxed),
            current_path: self
                .current_path
                .lock()
                .map(|s| s.clone())
                .unwrap_or_default(),
            elapsed_ms: self.started.elapsed().as_millis() as u64,
        }
    }
}

pub type SharedProgress = Arc<ScanProgress>;
