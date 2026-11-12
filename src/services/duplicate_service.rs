use std::sync::Arc;
use std::thread::JoinHandle;

use crossbeam_channel::{unbounded, Receiver};

use crate::analysis::duplicates::{find_duplicates, DuplicateGroup, DuplicateProgress};
use crate::core::models::ScanResult;

pub struct DuplicateService {
    progress: Arc<DuplicateProgress>,
    receiver: Receiver<Vec<DuplicateGroup>>,
    handle: Option<JoinHandle<()>>,
}

impl DuplicateService {
    pub fn start(scan: Arc<ScanResult>, min_size: u64) -> Self {
        let progress = Arc::new(DuplicateProgress::new());
        let (sender, receiver) = unbounded();
        let worker_progress = progress.clone();
        let handle = std::thread::Builder::new()
            .name("smartdisk-duplicates".into())
            .spawn(move || {
                let groups = find_duplicates(&scan, min_size, worker_progress);
                let _ = sender.send(groups);
            })
            .expect("failed to spawn duplicate thread");
        Self {
            progress,
            receiver,
            handle: Some(handle),
        }
    }

    pub fn progress(&self) -> Arc<DuplicateProgress> {
        self.progress.clone()
    }

    pub fn cancel(&self) {
        self.progress.cancel();
    }

    pub fn try_finish(&self) -> Option<Vec<DuplicateGroup>> {
        self.receiver.try_recv().ok()
    }

    pub fn is_finished(&self) -> bool {
        self.handle.as_ref().map(|h| h.is_finished()).unwrap_or(true)
    }
}

impl Drop for DuplicateService {
    fn drop(&mut self) {
        self.progress.cancel();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
