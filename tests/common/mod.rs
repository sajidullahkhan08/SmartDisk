//! Shared helpers for SmartDisk integration tests.
//!
//! Fixtures live under `.testdata/` in the project directory rather than the
//! system temp directory so classification and risk behave like a normal
//! user-data location instead of a recognized temporary folder.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use smartdisk::core::models::{NodeId, ScanResult};
use smartdisk::scanner::{run_scan, ScanControl, ScanOptions, ScanProgress};

pub fn scan(root: &Path) -> ScanResult {
    run_scan(
        ScanOptions::new(root.to_path_buf()),
        Arc::new(ScanControl::new()),
        Arc::new(ScanProgress::new()),
    )
    .expect("scan should succeed")
}

pub fn scan_with(root: &Path, configure: impl FnOnce(&mut ScanOptions)) -> ScanResult {
    let mut options = ScanOptions::new(root.to_path_buf());
    configure(&mut options);
    run_scan(
        options,
        Arc::new(ScanControl::new()),
        Arc::new(ScanProgress::new()),
    )
    .expect("scan should succeed")
}

pub fn write_file(path: &Path, bytes: usize) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture directory");
    }
    fs::write(path, vec![0u8; bytes]).expect("write fixture file");
}

/// Create a fresh, unique fixture root. Removes leftovers from an earlier run.
pub fn fixture_root(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".testdata")
        .join(format!("{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create fixture root");
    root
}

pub fn remove_fixture(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

pub fn find_child(result: &ScanResult, parent: NodeId, name: &str) -> Option<NodeId> {
    result
        .child_ids(parent)
        .into_iter()
        .find(|id| {
            result
                .node(*id)
                .map(|node| node.name.eq_ignore_ascii_case(name))
                .unwrap_or(false)
        })
}

pub fn find_path(result: &ScanResult, relative: &str) -> Option<NodeId> {
    // Accept either slash style in the relative path while comparing against
    // the scanner's backslash-normalized display paths.
    let mut wanted = PathBuf::from(&result.root_path);
    for part in relative.split(['\\', '/']).filter(|part| !part.is_empty()) {
        wanted.push(part);
    }
    let wanted = wanted.to_string_lossy();
    (1..result.nodes.len() as NodeId).find(|id| result.path_of(*id).eq_ignore_ascii_case(&wanted))
}
