use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use sha2::{Digest, Sha256};

use crate::core::models::{NodeId, ScanResult};
use crate::core::util::format_size;
use crate::platform::identity::identity_from_file;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DuplicateStage {
    Collecting,
    Hashing,
    Complete,
    Cancelled,
}

impl DuplicateStage {
    fn from_u8(v: u8) -> Self {
        match v {
            0 => DuplicateStage::Collecting,
            1 => DuplicateStage::Hashing,
            2 => DuplicateStage::Complete,
            _ => DuplicateStage::Cancelled,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            DuplicateStage::Collecting => "Collecting candidate files",
            DuplicateStage::Hashing => "Hashing file contents",
            DuplicateStage::Complete => "Complete",
            DuplicateStage::Cancelled => "Cancelled",
        }
    }
}

#[derive(Debug)]
pub struct DuplicateProgress {
    stage: AtomicU8,
    pub files_hashed: AtomicU64,
    pub bytes_hashed: AtomicU64,
    pub total_candidates: AtomicU64,
    pub groups_found: AtomicU64,
    pub errors: AtomicU64,
    cancel: AtomicBool,
    current: Mutex<String>,
    started: Instant,
}

impl Default for DuplicateProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl DuplicateProgress {
    pub fn new() -> Self {
        Self {
            stage: AtomicU8::new(0),
            files_hashed: AtomicU64::new(0),
            bytes_hashed: AtomicU64::new(0),
            total_candidates: AtomicU64::new(0),
            groups_found: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            cancel: AtomicBool::new(false),
            current: Mutex::new(String::new()),
            started: Instant::now(),
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
    pub fn stage(&self) -> DuplicateStage {
        DuplicateStage::from_u8(self.stage.load(Ordering::Relaxed))
    }
    fn set_stage(&self, s: DuplicateStage) {
        self.stage.store(
            match s {
                DuplicateStage::Collecting => 0,
                DuplicateStage::Hashing => 1,
                DuplicateStage::Complete => 2,
                DuplicateStage::Cancelled => 3,
            },
            Ordering::Relaxed,
        );
    }
    fn set_current(&self, text: String) {
        if let Ok(mut cur) = self.current.lock() {
            *cur = text;
        }
    }
    pub fn snapshot(&self) -> DuplicateSnapshot {
        DuplicateSnapshot {
            stage: self.stage().label().to_string(),
            files_hashed: self.files_hashed.load(Ordering::Relaxed),
            bytes_hashed: self.bytes_hashed.load(Ordering::Relaxed),
            total_candidates: self.total_candidates.load(Ordering::Relaxed),
            groups_found: self.groups_found.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            current: self.current.lock().map(|s| s.clone()).unwrap_or_default(),
            elapsed_ms: self.started.elapsed().as_millis() as u64,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DuplicateSnapshot {
    pub stage: String,
    pub files_hashed: u64,
    pub bytes_hashed: u64,
    pub total_candidates: u64,
    pub groups_found: u64,
    pub errors: u64,
    pub current: String,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug)]
pub struct DuplicateFile {
    pub node: NodeId,
    pub path: String,
    pub size: u64,
    pub identity: Option<(u32, u64)>,
    pub is_hard_link: bool,
    pub link_count: u32,
}

#[derive(Clone, Debug)]
pub struct DuplicateGroup {
    pub size: u64,
    pub hash: String,
    pub files: Vec<DuplicateFile>,
    /// Bytes that could be reclaimed by keeping one unique copy. Hard links to
    /// the same underlying file are counted as a single copy.
    pub recoverable: u64,
    pub hard_link_count: u64,
}

/// Files up to this size are hashed in full during the prefilter stage.
const PREFILTER_WHOLE_LIMIT: u64 = 128 * 1024;
/// Head/tail bytes hashed for files above the whole-file limit.
const PREFILTER_CHUNK: usize = 64 * 1024;
/// Chunk size for full-content hashing (reused per worker).
const FULL_BUFFER: usize = 1024 * 1024;

struct HashedFile {
    node: NodeId,
    path: String,
    identity: Option<(u32, u64)>,
    link_count: u32,
    digest: [u8; 32],
}

fn effective_workers() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8)
}

/// Run `f` over `items` on a small scoped thread pool. Each worker keeps its
/// own scratch buffer so no per-file allocation happens on the hot path.
/// Results preserve the input order (as `None` for items skipped by `f`).
fn parallel_map<T, R, F>(items: &[T], workers: usize, f: F) -> Vec<Option<R>>
where
    T: Sync,
    R: Send,
    F: Fn(&T, &mut Vec<u8>) -> R + Sync,
{
    if items.is_empty() {
        return Vec::new();
    }
    let workers = workers.clamp(1, items.len());
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                let mut buf: Vec<u8> = Vec::new();
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= items.len() {
                        break;
                    }
                    let value = f(&items[i], &mut buf);
                    match results.lock() {
                        Ok(mut slots) => slots[i] = Some(value),
                        Err(poisoned) => {
                            poisoned.into_inner()[i] = Some(value);
                        }
                    }
                }
            });
        }
    });
    match results.into_inner() {
        Ok(slots) => slots,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn digest_hex(digest: &[u8; 32]) -> String {
    let mut out = String::with_capacity(16);
    for b in digest.iter().take(8) {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Cheap first pass: hash the whole file when small, otherwise the first and
/// last 64 KiB. Files that differ in these regions are ruled out without
/// reading their full contents.
fn hash_prefilter(
    file: &mut File,
    size: u64,
    buf: &mut Vec<u8>,
) -> std::io::Result<([u8; 32], u64)> {
    let mut hasher = Sha256::new();
    let mut bytes = 0u64;
    if size <= PREFILTER_WHOLE_LIMIT {
        buf.resize(PREFILTER_WHOLE_LIMIT as usize, 0);
        let mut filled = 0usize;
        while filled < buf.len() {
            let n = file.read(&mut buf[filled..])?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        hasher.update(&buf[..filled]);
        bytes += filled as u64;
    } else {
        buf.resize(PREFILTER_CHUNK, 0);
        let mut head = 0usize;
        while head < PREFILTER_CHUNK {
            let n = file.read(&mut buf[head..])?;
            if n == 0 {
                break;
            }
            head += n;
        }
        hasher.update(&buf[..head]);
        bytes += head as u64;
        file.seek(SeekFrom::Start(size - PREFILTER_CHUNK as u64))?;
        let mut tail = 0usize;
        while tail < PREFILTER_CHUNK {
            let n = file.read(&mut buf[tail..])?;
            if n == 0 {
                break;
            }
            tail += n;
        }
        hasher.update(&buf[..tail]);
        bytes += tail as u64;
    }
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    Ok((out, bytes))
}

fn hash_full(
    file: &mut File,
    buf: &mut Vec<u8>,
    progress: &DuplicateProgress,
) -> std::io::Result<Option<[u8; 32]>> {
    buf.resize(FULL_BUFFER, 0);
    let mut hasher = Sha256::new();
    loop {
        if progress.is_cancelled() {
            return Ok(None);
        }
        let n = file.read(buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        progress.bytes_hashed.fetch_add(n as u64, Ordering::Relaxed);
    }
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    Ok(Some(out))
}

fn hash_candidate(
    id: NodeId,
    path: &str,
    size: u64,
    progress: &DuplicateProgress,
    buf: &mut Vec<u8>,
) -> Option<HashedFile> {
    if progress.is_cancelled() {
        return None;
    }
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => {
            progress.errors.fetch_add(1, Ordering::Relaxed);
            return None;
        }
    };
    let info = identity_from_file(&file);
    let identity = info.map(|i| (i.volume, i.index));
    let link_count = info.map(|i| i.links).unwrap_or(0);
    match hash_prefilter(&mut file, size, buf) {
        Ok((digest, bytes)) => {
            progress.bytes_hashed.fetch_add(bytes, Ordering::Relaxed);
            progress.files_hashed.fetch_add(1, Ordering::Relaxed);
            Some(HashedFile {
                node: id,
                path: path.to_string(),
                identity,
                link_count,
                digest,
            })
        }
        Err(_) => {
            progress.errors.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
}

fn hash_full_candidate(
    h: &HashedFile,
    buf: &mut Vec<u8>,
    progress: &DuplicateProgress,
) -> Option<[u8; 32]> {
    if progress.is_cancelled() {
        return None;
    }
    let mut file = match File::open(&h.path) {
        Ok(f) => f,
        Err(_) => {
            progress.errors.fetch_add(1, Ordering::Relaxed);
            return None;
        }
    };
    match hash_full(&mut file, buf, progress) {
        Ok(Some(digest)) => Some(digest),
        Ok(None) => None,
        Err(_) => {
            progress.errors.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
}

/// All candidates resolve to the same underlying file (hard links/aliases).
fn all_same_identity(files: &[HashedFile]) -> bool {
    match files.first().and_then(|f| f.identity) {
        Some(first) => files.iter().all(|f| f.identity == Some(first)),
        None => false,
    }
}

fn build_groups(size: u64, files: Vec<HashedFile>) -> Option<DuplicateGroup> {
    if files.len() < 2 {
        return None;
    }
    let hash = digest_hex(&files[0].digest);
    let mut identity_counts: HashMap<(u32, u64), u64> = HashMap::new();
    let mut synthetic = 1u64;
    for f in &files {
        let key = match f.identity {
            Some(id) => id,
            None => {
                // No stable identity available: treat each path as unique.
                let key = (u32::MAX, synthetic);
                synthetic += 1;
                key
            }
        };
        *identity_counts.entry(key).or_default() += 1;
    }
    let unique = identity_counts.len() as u64;
    let mut hard_link_count = 0u64;
    let mut out = Vec::with_capacity(files.len());
    for f in files {
        let shared = f
            .identity
            .and_then(|id| identity_counts.get(&id).copied())
            .unwrap_or(0)
            > 1;
        let is_hard_link = f.link_count > 1 || shared;
        if is_hard_link {
            hard_link_count += 1;
        }
        out.push(DuplicateFile {
            node: f.node,
            path: f.path,
            size,
            identity: f.identity,
            is_hard_link,
            link_count: f.link_count,
        });
    }
    Some(DuplicateGroup {
        size,
        hash,
        files: out,
        recoverable: size.saturating_mul(unique.saturating_sub(1)),
        hard_link_count,
    })
}

/// Find duplicate files. Files are grouped by exact size, then passed through
/// a parallel two-stage content check: a cheap head/tail prefilter rules out
/// most candidates, and a full SHA-256 is only computed for prefilter
/// collisions. Hard links to the same file are detected by file identity and
/// never reported as separate recoverable copies.
/// Cancellation leaves the result empty; partial duplicate data is never
/// presented as complete.
pub fn find_duplicates(
    result: &ScanResult,
    min_size: u64,
    progress: Arc<DuplicateProgress>,
) -> Vec<DuplicateGroup> {
    progress.set_stage(DuplicateStage::Collecting);
    let mut by_size: HashMap<u64, Vec<NodeId>> = HashMap::new();
    for id in 1..result.nodes.len() as NodeId {
        let node = &result.nodes[id as usize];
        if node.is_dir || node.size < min_size {
            continue;
        }
        by_size.entry(node.size).or_default().push(id);
    }
    let mut candidate_groups: Vec<(u64, Vec<NodeId>)> = by_size
        .into_iter()
        .filter(|(_, ids)| ids.len() > 1)
        .collect();
    candidate_groups.sort_by(|a, b| b.0.cmp(&a.0));
    let total: u64 = candidate_groups.iter().map(|(_, ids)| ids.len() as u64).sum();
    progress.total_candidates.store(total, Ordering::Relaxed);

    if progress.is_cancelled() {
        progress.set_stage(DuplicateStage::Cancelled);
        return Vec::new();
    }

    progress.set_stage(DuplicateStage::Hashing);
    let workers = effective_workers();
    let mut groups = Vec::new();
    for (size, ids) in candidate_groups {
        if progress.is_cancelled() {
            progress.set_stage(DuplicateStage::Cancelled);
            return Vec::new();
        }
        let candidates: Vec<(NodeId, String)> =
            ids.iter().map(|&id| (id, result.path_of(id))).collect();
        progress.set_current(format!(
            "{} candidate(s) of {}",
            candidates.len(),
            format_size(size)
        ));
        let prefiltered = parallel_map(&candidates, workers, |(id, path), buf| {
            hash_candidate(*id, path, size, &progress, buf)
        });
        let hashed: Vec<HashedFile> = prefiltered.into_iter().flatten().flatten().collect();
        if hashed.len() < 2 {
            continue;
        }
        if progress.is_cancelled() {
            progress.set_stage(DuplicateStage::Cancelled);
            return Vec::new();
        }

        let mut buckets: HashMap<[u8; 32], Vec<HashedFile>> = HashMap::new();
        for h in hashed {
            buckets.entry(h.digest).or_default().push(h);
        }
        for bucket in buckets.into_values() {
            if bucket.len() < 2 {
                continue;
            }
            if progress.is_cancelled() {
                progress.set_stage(DuplicateStage::Cancelled);
                return Vec::new();
            }
            // Hard links (or aliases) to one underlying file are already a
            // verified duplicate set; no full read is required.
            if all_same_identity(&bucket) {
                if let Some(group) = build_groups(size, bucket) {
                    groups.push(group);
                }
                continue;
            }
            let refs: Vec<&HashedFile> = bucket.iter().collect();
            let full =
                parallel_map(&refs, workers, |h, buf| hash_full_candidate(h, buf, &progress));
            let mut verified: HashMap<[u8; 32], Vec<HashedFile>> = HashMap::new();
            for (h, digest) in bucket.into_iter().zip(full) {
                if let Some(Some(digest)) = digest {
                    verified.entry(digest).or_default().push(h);
                }
            }
            for files in verified.into_values() {
                if let Some(group) = build_groups(size, files) {
                    groups.push(group);
                }
            }
        }
    }

    if progress.is_cancelled() {
        progress.set_stage(DuplicateStage::Cancelled);
        return Vec::new();
    }
    groups.sort_by(|a, b| b.recoverable.cmp(&a.recoverable).then(b.size.cmp(&a.size)));
    progress.groups_found.store(groups.len() as u64, Ordering::Relaxed);
    progress.set_stage(DuplicateStage::Complete);
    progress.set_current(String::new());
    groups
}

pub fn describe_group(group: &DuplicateGroup) -> String {
    let count = group.files.len();
    format!(
        "{} {}, {} each, up to {} recoverable",
        count,
        if count == 1 { "file" } else { "files" },
        format_size(group.size),
        format_size(group.recoverable)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{run_scan, ScanControl, ScanOptions, ScanProgress};
    use std::fs;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn scan(dir: &std::path::Path) -> ScanResult {
        run_scan(
            ScanOptions::new(dir.to_path_buf()),
            Arc::new(ScanControl::new()),
            Arc::new(ScanProgress::new()),
        )
        .unwrap()
    }

    #[test]
    fn finds_content_duplicates_not_just_sizes() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.bin"), vec![7u8; 4096]).unwrap();
        fs::write(dir.path().join("b.bin"), vec![7u8; 4096]).unwrap();
        fs::write(dir.path().join("c.bin"), vec![8u8; 4096]).unwrap();
        fs::write(dir.path().join("d.bin"), vec![8u8; 4096]).unwrap();
        fs::write(dir.path().join("unique.bin"), vec![9u8; 100]).unwrap();
        let result = scan(dir.path());
        let progress = Arc::new(DuplicateProgress::new());
        let groups = find_duplicates(&result, 1, progress);
        // Size alone would produce one group of four files; content hashing
        // must split them into two groups of two.
        assert_eq!(groups.len(), 2);
        assert!(groups.iter().all(|g| g.files.len() == 2));
        assert!(groups.iter().all(|g| g.recoverable == 4096));
        assert!(groups.iter().all(|g| g.files.iter().all(|f| !f.is_hard_link)));
    }

    #[test]
    fn hard_links_are_not_counted_as_recoverable_copies() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("original.bin"), vec![3u8; 8192]).unwrap();
        fs::hard_link(
            dir.path().join("original.bin"),
            dir.path().join("hardlink.bin"),
        )
        .unwrap();
        let result = scan(dir.path());
        let progress = Arc::new(DuplicateProgress::new());
        let groups = find_duplicates(&result, 1, progress);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2);
        assert_eq!(groups[0].recoverable, 0);
        assert!(groups[0].files.iter().all(|f| f.is_hard_link));
    }

    #[test]
    fn cancellation_produces_no_partial_result() {
        let dir = tempdir().unwrap();
        for i in 0..20 {
            fs::write(dir.path().join(format!("f{i}.bin")), vec![1u8; 50_000]).unwrap();
        }
        let result = scan(dir.path());
        let progress = Arc::new(DuplicateProgress::new());
        progress.cancel();
        let groups = find_duplicates(&result, 1, progress.clone());
        assert!(groups.is_empty());
        assert_eq!(progress.stage(), DuplicateStage::Cancelled);
    }

    #[test]
    fn large_files_with_same_edges_but_different_content_are_not_duplicates() {
        let dir = tempdir().unwrap();
        let size = 600 * 1024;
        let mut a = vec![5u8; size];
        let mut b = vec![5u8; size];
        // Identical head (64 KiB) and tail (64 KiB), different middle.
        a[size / 2] = 1;
        b[size / 2] = 2;
        fs::write(dir.path().join("edge-a.bin"), &a).unwrap();
        fs::write(dir.path().join("edge-b.bin"), &b).unwrap();
        let result = scan(dir.path());
        let progress = Arc::new(DuplicateProgress::new());
        let groups = find_duplicates(&result, 1, progress);
        assert!(groups.is_empty(), "candidates must be verified by full hash");
    }

    #[test]
    fn large_identical_files_are_found() {
        let dir = tempdir().unwrap();
        let size = 300 * 1024;
        fs::write(dir.path().join("big-a.bin"), vec![6u8; size]).unwrap();
        fs::write(dir.path().join("big-b.bin"), vec![6u8; size]).unwrap();
        let result = scan(dir.path());
        let progress = Arc::new(DuplicateProgress::new());
        let groups = find_duplicates(&result, 1, progress);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 2);
        assert_eq!(groups[0].recoverable, size as u64);
    }
}
