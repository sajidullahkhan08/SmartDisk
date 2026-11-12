use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

/// Cooperative cancellation and pause control shared with scanner workers.
#[derive(Debug, Default)]
pub struct ScanControl {
    cancel: AtomicBool,
    paused: Mutex<bool>,
    cv: Condvar,
}

impl ScanControl {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pause(&self) {
        if let Ok(mut p) = self.paused.lock() {
            *p = true;
        }
    }

    pub fn resume(&self) {
        if let Ok(mut p) = self.paused.lock() {
            *p = false;
        }
        self.cv.notify_all();
    }

    pub fn is_paused(&self) -> bool {
        self.paused.lock().map(|p| *p).unwrap_or(false)
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.resume();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// Block while paused. Returns true when cancellation was requested.
    pub fn wait_if_paused(&self) -> bool {
        if self.is_cancelled() {
            return true;
        }
        let Ok(mut paused) = self.paused.lock() else {
            return self.is_cancelled();
        };
        while *paused && !self.is_cancelled() {
            let (guard, _) = self
                .cv
                .wait_timeout(paused, Duration::from_millis(100))
                .unwrap_or_else(|e| e.into_inner());
            paused = guard;
        }
        self.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Instant;

    #[test]
    fn pause_and_cancel_wake_waiters() {
        let control = Arc::new(ScanControl::new());
        control.pause();
        let c2 = control.clone();
        let handle = thread::spawn(move || c2.wait_if_paused());
        thread::sleep(Duration::from_millis(80));
        assert!(!handle.is_finished());
        control.cancel();
        let started = Instant::now();
        assert!(handle.join().unwrap());
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
