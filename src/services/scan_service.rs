use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;

use crossbeam_channel::{unbounded, Receiver};

use crate::core::errors::ScanError;
use crate::core::models::ScanResult;
use crate::scanner::control::ScanControl;
use crate::scanner::engine::{run_scan, ScanOptions};
use crate::scanner::progress::{ScanProgress, SharedProgress};

pub struct ScanService {
    control: Arc<ScanControl>,
    progress: SharedProgress,
    receiver: Receiver<Result<ScanResult, ScanError>>,
    handle: Option<JoinHandle<()>>,
    root: PathBuf,
}

impl ScanService {
    pub fn start(options: ScanOptions) -> Self {
        let root = options.root.clone();
        let control = Arc::new(ScanControl::new());
        let progress = Arc::new(ScanProgress::new());
        let (sender, receiver) = unbounded();
        let worker_control = control.clone();
        let worker_progress = progress.clone();
        let handle = std::thread::Builder::new()
            .name("smartdisk-scan-coordinator".into())
            .spawn(move || {
                let result = run_scan(options, worker_control, worker_progress);
                let _ = sender.send(result);
            })
            .expect("failed to spawn scan thread");
        Self {
            control,
            progress,
            receiver,
            handle: Some(handle),
            root,
        }
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    pub fn progress(&self) -> SharedProgress {
        self.progress.clone()
    }

    pub fn pause(&self) {
        self.control.pause();
    }

    pub fn resume(&self) {
        self.control.resume();
    }

    pub fn is_paused(&self) -> bool {
        self.control.is_paused()
    }

    pub fn cancel(&self) {
        self.control.cancel();
    }

    /// Non-blocking check for the final result. `None` while the scan runs.
    pub fn try_finish(&self) -> Option<Result<ScanResult, ScanError>> {
        self.receiver.try_recv().ok()
    }

    pub fn is_finished(&self) -> bool {
        self.handle.as_ref().map(|h| h.is_finished()).unwrap_or(true)
    }
}

impl Drop for ScanService {
    fn drop(&mut self) {
        self.control.cancel();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
