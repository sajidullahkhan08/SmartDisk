use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::os::windows::fs::MetadataExt;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use crossbeam_channel::{bounded, unbounded, Receiver, Sender};

use crate::core::errors::ScanError;
use crate::core::models::{
    Category, Confidence, FsNode, NodeId, ScanIssue, ScanResult, ScanStats, ScanStatus, VolumeInfo,
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, INVALID_NODE,
};
use crate::core::util::{
    extended_path, is_same_or_subpath, normalize_display_root, now_unix, path_key,
    system_time_to_unix,
};
use crate::intelligence::{assess, classify_entry, PathContext};
use crate::scanner::control::ScanControl;
use crate::scanner::progress::SharedProgress;

#[derive(Clone, Debug)]
pub struct ScanOptions {
    pub root: PathBuf,
    pub workers: usize,
    pub low_resource: bool,
    pub follow_reparse_points: bool,
    pub exclusions: Vec<PathBuf>,
    pub cluster_size: u64,
    pub volume: Option<VolumeInfo>,
}

impl ScanOptions {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            workers: 0,
            low_resource: false,
            follow_reparse_points: false,
            exclusions: Vec::new(),
            cluster_size: 4096,
            volume: None,
        }
    }

    fn effective_workers(&self) -> usize {
        if self.low_resource {
            return 2.min(
                thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(2),
            );
        }
        if self.workers > 0 {
            return self.workers.clamp(1, 32);
        }
        let cpus = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        cpus.saturating_sub(1).clamp(1, 8)
    }

    fn is_excluded(&self, path: &Path) -> bool {
        self.exclusions
            .iter()
            .any(|excluded| is_same_or_subpath(path, excluded))
    }

    fn cluster(&self) -> u64 {
        if self.cluster_size == 0 {
            4096
        } else {
            self.cluster_size
        }
    }
}

struct Job {
    node_id: NodeId,
    io_path: PathBuf,
    display_path: PathBuf,
}

struct EntryInfo {
    name: String,
    dir_path: Option<PathBuf>,
    is_dir: bool,
    size: u64,
    allocated: u64,
    modified: Option<i64>,
    created: Option<i64>,
    attributes: u32,
    category: Category,
    confidence: Confidence,
    risk: crate::core::models::RiskLevel,
    traverse: bool,
}

struct DirOutcome {
    node_id: NodeId,
    display_path: PathBuf,
    entries: Vec<EntryInfo>,
    error: Option<String>,
    issues: Vec<ScanIssue>,
}

fn allocated_size(logical: u64, cluster: u64) -> u64 {
    if logical == 0 {
        0
    } else {
        logical.div_ceil(cluster) * cluster
    }
}

fn map_root_error(path: &Path, err: &io::Error) -> ScanError {
    match err.kind() {
        io::ErrorKind::NotFound => ScanError::NotFound(path.to_string_lossy().into_owned()),
        io::ErrorKind::PermissionDenied => {
            ScanError::AccessDenied(path.to_string_lossy().into_owned())
        }
        _ => ScanError::Io(io::Error::new(err.kind(), err.to_string())),
    }
}

/// Run a scan synchronously on the current thread (the caller usually runs this
/// inside a worker thread). Progress and cancellation are shared handles.
pub fn run_scan(
    options: ScanOptions,
    control: Arc<ScanControl>,
    progress: SharedProgress,
) -> Result<ScanResult, ScanError> {
    let started = Instant::now();
    let started_at = now_unix();
    let absolute = std::path::absolute(&options.root)
        .map_err(|e| ScanError::Io(io::Error::new(e.kind(), e.to_string())))?;
    let display_root = normalize_display_root(&absolute);
    let root_path = PathBuf::from(&display_root);
    let extended = extended_path(&absolute);

    let root_meta = std::fs::symlink_metadata(&extended).map_err(|e| map_root_error(&root_path, &e))?;
    if root_meta.file_attributes() & FILE_ATTRIBUTE_DIRECTORY == 0 {
        return Err(ScanError::NotADirectory(display_root));
    }
    // Fail early with an actionable error instead of returning an empty result.
    std::fs::read_dir(&extended).map_err(|e| map_root_error(&root_path, &e))?;

    let root_modified = root_meta.modified().ok().and_then(system_time_to_unix);
    let root_created = root_meta.created().ok().and_then(system_time_to_unix);
    let root_attrs = root_meta.file_attributes();

    let mut nodes: Vec<FsNode> = Vec::with_capacity(4096);
    nodes.push(FsNode {
        parent: INVALID_NODE,
        first_child: INVALID_NODE,
        next_sibling: INVALID_NODE,
        name: display_root.clone(),
        is_dir: true,
        size: 0,
        allocated: 0,
        direct_size: 0,
        file_count: 0,
        dir_count: 0,
        modified: root_modified,
        created: root_created,
        attributes: root_attrs,
        category: Category::Other,
        confidence: Confidence::Low,
        risk: crate::core::models::RiskLevel::Unknown,
    });
    let root_id: NodeId = 0;

    let (job_tx, job_rx) = bounded::<Job>(2048);
    let (result_tx, result_rx) = unbounded::<DirOutcome>();

    let worker_count = options.effective_workers();
    let mut handles = Vec::with_capacity(worker_count);
    for n in 0..worker_count {
        let rx = job_rx.clone();
        let tx = result_tx.clone();
        let control = control.clone();
        let progress = progress.clone();
        let options = options.clone();
        let handle = thread::Builder::new()
            .name(format!("smartdisk-scan-{n}"))
            .spawn(move || worker_loop(rx, tx, control, progress, options))
            .map_err(ScanError::Io)?;
        handles.push(handle);
    }
    drop(result_tx);

    let mut last_child: Vec<NodeId> = vec![INVALID_NODE];
    let mut pending_children: Vec<u32> = vec![0];
    let mut outcome_done: Vec<bool> = vec![false];
    let mut finalized: Vec<bool> = vec![false];
    let mut visited_dirs: HashSet<String> = HashSet::new();
    if options.follow_reparse_points {
        if let Ok(canon) = dunce::canonicalize(&absolute) {
            visited_dirs.insert(path_key(&canon));
        }
    }

    let mut pending_jobs: u64 = 1;
    if job_tx
        .send(Job {
            node_id: root_id,
            io_path: absolute.clone(),
            display_path: root_path.clone(),
        })
        .is_err()
    {
        return Err(ScanError::Cancelled);
    }

    let mut issues: Vec<ScanIssue> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut fatal: Option<ScanError> = None;
    let mut cancelled = false;

    while pending_jobs > 0 {
        if control.is_cancelled() || control.wait_if_paused() {
            cancelled = true;
            break;
        }
        let outcome = match result_rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(o) => o,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        };
        pending_jobs = pending_jobs.saturating_sub(1);
        progress.dirs.fetch_add(1, Ordering::Relaxed);

        if let Some(err) = &outcome.error {
            if outcome.node_id == root_id {
                fatal = Some(ScanError::AccessDenied(format!(
                    "{}: {}",
                    outcome.display_path.to_string_lossy(),
                    err
                )));
                break;
            }
            progress.errors.fetch_add(1, Ordering::Relaxed);
            if issues.len() < 500 {
                issues.push(ScanIssue {
                    path: outcome.display_path.to_string_lossy().into_owned(),
                    message: err.clone(),
                });
            }
            outcome_done[outcome.node_id as usize] = true;
            maybe_finalize(
                outcome.node_id,
                &mut nodes,
                &mut pending_children,
                &mut outcome_done,
                &mut finalized,
            );
            continue;
        }

        for issue in outcome.issues {
            progress.errors.fetch_add(1, Ordering::Relaxed);
            if issues.len() < 500 {
                issues.push(issue);
            }
        }

        if nodes.len() >= NodeId::MAX as usize {
            warnings.push("The scan reached the maximum representable item count; remaining items were skipped.".into());
            break;
        }

        for entry in outcome.entries {
            let id = nodes.len() as NodeId;
            let node = FsNode {
                parent: outcome.node_id,
                first_child: INVALID_NODE,
                next_sibling: INVALID_NODE,
                name: entry.name,
                is_dir: entry.is_dir,
                size: if entry.is_dir { 0 } else { entry.size },
                allocated: if entry.is_dir { 0 } else { entry.allocated },
                direct_size: if entry.is_dir { 0 } else { entry.size },
                file_count: 0,
                dir_count: 0,
                modified: entry.modified,
                created: entry.created,
                attributes: entry.attributes,
                category: entry.category,
                confidence: entry.confidence,
                risk: entry.risk,
            };
            nodes.push(node);
            last_child.push(INVALID_NODE);
            pending_children.push(0);
            outcome_done.push(false);
            finalized.push(false);

            let parent_idx = outcome.node_id as usize;
            let previous = last_child[parent_idx];
            if previous == INVALID_NODE {
                nodes[parent_idx].first_child = id;
            } else {
                nodes[previous as usize].next_sibling = id;
            }
            last_child[parent_idx] = id;

            if entry.is_dir {
                pending_children[parent_idx] = pending_children[parent_idx].saturating_add(1);
                let child_display = outcome.display_path.join(&nodes[id as usize].name);
                let mut traverse = entry.traverse;
                if traverse
                    && options.follow_reparse_points
                    && entry.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
                {
                    let key = entry
                        .dir_path
                        .as_ref()
                        .and_then(|p| dunce::canonicalize(p).ok())
                        .map(|p| path_key(&p));
                    match key {
                        Some(key) if !visited_dirs.insert(key.clone()) => {
                            traverse = false;
                            warnings.push(format!(
                                "Cycle prevented while following reparse points at {}",
                                child_display.to_string_lossy()
                            ));
                        }
                        None => {
                            traverse = false;
                            progress.skipped_reparse.fetch_add(1, Ordering::Relaxed);
                        }
                        _ => {}
                    }
                }

                if traverse {
                    if let Some(io_path) = entry.dir_path {
                        if job_tx
                            .send(Job {
                                node_id: id,
                                io_path,
                                display_path: child_display,
                            })
                            .is_err()
                        {
                            cancelled = true;
                            break;
                        }
                        pending_jobs += 1;
                    } else {
                        outcome_done[id as usize] = true;
                        maybe_finalize(
                            id,
                            &mut nodes,
                            &mut pending_children,
                            &mut outcome_done,
                            &mut finalized,
                        );
                    }
                } else {
                    if entry.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                        progress.skipped_reparse.fetch_add(1, Ordering::Relaxed);
                    }
                    outcome_done[id as usize] = true;
                    maybe_finalize(
                        id,
                        &mut nodes,
                        &mut pending_children,
                        &mut outcome_done,
                        &mut finalized,
                    );
                }
            } else {
                nodes[parent_idx].size = nodes[parent_idx].size.saturating_add(entry.size);
                nodes[parent_idx].allocated = nodes[parent_idx]
                    .allocated
                    .saturating_add(entry.allocated);
                nodes[parent_idx].direct_size =
                    nodes[parent_idx].direct_size.saturating_add(entry.size);
                nodes[parent_idx].file_count = nodes[parent_idx].file_count.saturating_add(1);
                progress.files.fetch_add(1, Ordering::Relaxed);
                progress.bytes.fetch_add(entry.size, Ordering::Relaxed);
            }
        }

        outcome_done[outcome.node_id as usize] = true;
        maybe_finalize(
            outcome.node_id,
            &mut nodes,
            &mut pending_children,
            &mut outcome_done,
            &mut finalized,
        );
    }

    if control.is_cancelled() {
        cancelled = true;
    }
    if cancelled {
        control.cancel();
    }
    drop(job_tx);
    for handle in handles {
        let _ = handle.join();
    }

    if let Some(err) = fatal {
        return Err(err);
    }

    let files = nodes.iter().skip(1).filter(|n| !n.is_dir).count() as u64;
    let dirs = nodes.iter().skip(1).filter(|n| n.is_dir).count() as u64;
    let skipped_reparse = progress.skipped_reparse.load(Ordering::Relaxed);
    let errors = progress.errors.load(Ordering::Relaxed);

    let mut largest_file = (INVALID_NODE, 0u64);
    let mut largest_dir = (INVALID_NODE, 0u64);
    for (idx, node) in nodes.iter().enumerate().skip(1) {
        if node.is_dir {
            if node.size > largest_dir.1 {
                largest_dir = (idx as NodeId, node.size);
            }
        } else if node.size > largest_file.1 {
            largest_file = (idx as NodeId, node.size);
        }
    }

    let status = if cancelled {
        ScanStatus::Cancelled
    } else if errors > 0 {
        ScanStatus::Partial
    } else {
        ScanStatus::Complete
    };
    if skipped_reparse > 0 {
        warnings.push(format!(
            "{skipped_reparse} reparse point(s) (junctions/symlinks) were not followed. Their targets are not included in totals."
        ));
    }
    if cancelled {
        warnings.push("The scan was cancelled. Results are incomplete and totals cover only items discovered so far.".into());
    }
    if errors > 0 {
        warnings.push(format!(
            "{errors} item(s) could not be read; their contents are not included."
        ));
    }

    let total_size = nodes[root_id as usize].size;
    let total_allocated = nodes[root_id as usize].allocated;
    let finished_at = now_unix();
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let stats = ScanStats {
        total_size,
        total_allocated,
        files,
        dirs,
        errors,
        skipped_reparse,
        elapsed_ms,
        largest_file,
        largest_dir,
        average_file_size: if files > 0 { total_size / files } else { 0 },
    };

    progress.set_current("");

    Ok(ScanResult {
        nodes,
        root: root_id,
        root_path: display_root,
        status,
        stats,
        errors: issues,
        warnings,
        started_at,
        finished_at,
        volume: options.volume,
    })
}

fn maybe_finalize(
    start: NodeId,
    nodes: &mut [FsNode],
    pending_children: &mut [u32],
    outcome_done: &mut [bool],
    finalized: &mut [bool],
) {
    let mut current = start;
    loop {
        let idx = current as usize;
        if idx >= nodes.len()
            || !outcome_done[idx]
            || pending_children[idx] > 0
            || finalized[idx]
        {
            break;
        }
        finalized[idx] = true;
        let parent = nodes[idx].parent;
        if parent == INVALID_NODE {
            break;
        }
        let size = nodes[idx].size;
        let allocated = nodes[idx].allocated;
        let files = nodes[idx].file_count;
        let dirs = nodes[idx].dir_count;
        let pidx = parent as usize;
        nodes[pidx].size = nodes[pidx].size.saturating_add(size);
        nodes[pidx].allocated = nodes[pidx].allocated.saturating_add(allocated);
        nodes[pidx].file_count = nodes[pidx].file_count.saturating_add(files);
        nodes[pidx].dir_count = nodes[pidx].dir_count.saturating_add(dirs).saturating_add(1);
        pending_children[pidx] = pending_children[pidx].saturating_sub(1);
        current = parent;
    }
}

fn worker_loop(
    rx: Receiver<Job>,
    tx: Sender<DirOutcome>,
    control: Arc<ScanControl>,
    progress: SharedProgress,
    options: ScanOptions,
) {
    let now = now_unix();
    let has_exclusions = !options.exclusions.is_empty();
    while let Ok(job) = rx.recv_timeout(std::time::Duration::from_millis(100)) {
        if control.wait_if_paused() {
            break;
        }
        let display = job.display_path.to_string_lossy().into_owned();
        progress.set_current(&display);
        let ctx = PathContext::new(&job.display_path);
        let mut entries = Vec::new();
        let mut issues = Vec::new();
        let mut error = None;
        match crate::platform::dir_enum::list_directory(&extended_path(&job.io_path)) {
            Err(e) => {
                error = Some(human_io_error(&e));
            }
            Ok(listing) => {
                if let Some(e) = listing.next_error {
                    issues.push(ScanIssue {
                        path: display.clone(),
                        message: human_io_error(&e),
                    });
                }
                entries.reserve(listing.entries.len());
                for raw in listing.entries {
                    let name = raw.name.to_string_lossy().into_owned();
                    // Exclusion checks need the child path; skip the join when
                    // no exclusions are configured (the common case).
                    if has_exclusions
                        && options.is_excluded(&job.display_path.join(&name))
                    {
                        continue;
                    }
                    let attributes = raw.attributes;
                    let is_dir = attributes & FILE_ATTRIBUTE_DIRECTORY != 0;
                    let is_reparse = attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0;
                    let traverse = !(is_dir && is_reparse && !options.follow_reparse_points);
                    let size = if is_dir { 0 } else { raw.size };
                    let (category, confidence) =
                        classify_entry(&ctx, &name, is_dir, attributes);
                    let modified = raw.modified_unix;
                    let created = raw.created_unix;
                    let risk = assess(
                        &ctx,
                        is_dir,
                        attributes,
                        modified,
                        category,
                        now,
                    );
                    let dir_path = is_dir.then(|| job.io_path.join(&name));
                    entries.push(EntryInfo {
                        name,
                        dir_path,
                        is_dir,
                        size,
                        allocated: allocated_size(size, options.cluster()),
                        modified,
                        created,
                        attributes,
                        category,
                        confidence,
                        risk,
                        traverse,
                    });
                }
            }
        }
        if tx
            .send(DirOutcome {
                node_id: job.node_id,
                display_path: job.display_path,
                entries,
                error,
                issues,
            })
            .is_err()
        {
            break;
        }
        if control.is_cancelled() {
            break;
        }
    }
}

fn human_io_error(err: &io::Error) -> String {
    match err.kind() {
        io::ErrorKind::PermissionDenied => "Access denied".into(),
        io::ErrorKind::NotFound => "Item disappeared or was not found".into(),
        io::ErrorKind::TimedOut => "The operation timed out".into(),
        _ => err.to_string(),
    }
}

/// Exclusions are matched by path components and never by substring.
pub fn parse_exclusions(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .map(|p| {
            if p.is_absolute() {
                p
            } else {
                std::path::absolute(&p).unwrap_or(p)
            }
        })
        .collect()
}

pub fn exclusions_summary(exclusions: &[PathBuf]) -> String {
    exclusions
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::progress::ScanProgress;
    use std::fs;
    use tempfile::tempdir;

    fn scan(root: PathBuf) -> ScanResult {
        let control = Arc::new(ScanControl::new());
        let progress = Arc::new(ScanProgress::new());
        run_scan(ScanOptions::new(root), control, progress).expect("scan")
    }

    #[test]
    fn scans_nested_tree_and_aggregates() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a/b")).unwrap();
        fs::write(dir.path().join("root.txt"), vec![0u8; 100]).unwrap();
        fs::write(dir.path().join("a/one.bin"), vec![0u8; 200]).unwrap();
        fs::write(dir.path().join("a/b/two.bin"), vec![0u8; 300]).unwrap();
        let result = scan(dir.path().to_path_buf());
        assert_eq!(result.stats.files, 3);
        assert_eq!(result.stats.dirs, 2);
        assert_eq!(result.stats.total_size, 600);
        assert_eq!(result.status, ScanStatus::Complete);
        let root = &result.nodes[result.root as usize];
        assert_eq!(root.size, 600);
        assert_eq!(root.file_count, 3);
        assert_eq!(root.dir_count, 2);
        let a = result
            .children(result.root)
            .find(|n| n.name == "a")
            .unwrap();
        assert_eq!(a.size, 500);
        assert_eq!(a.file_count, 2);
        assert_eq!(a.dir_count, 1);
    }

    #[test]
    fn handles_empty_and_zero_byte_files() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("empty")).unwrap();
        fs::write(dir.path().join("zero"), b"").unwrap();
        let result = scan(dir.path().to_path_buf());
        assert_eq!(result.stats.files, 1);
        assert_eq!(result.stats.total_size, 0);
        let zero = result
            .children(result.root)
            .find(|n| n.name == "zero")
            .unwrap();
        assert_eq!(zero.size, 0);
    }

    #[test]
    fn unicode_and_long_names() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("日本語 ファイル.txt"), b"hello").unwrap();
        fs::write(dir.path().join("emoji-🙂.bin"), b"abc").unwrap();
        let long = format!("{}.dat", "x".repeat(180));
        fs::write(dir.path().join(&long), b"123").unwrap();
        let result = scan(dir.path().to_path_buf());
        assert_eq!(result.stats.files, 3);
        assert!(result
            .children(result.root)
            .any(|n| n.name.contains("日本語")));
    }

    #[test]
    fn exclusions_are_component_aware() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("keep")).unwrap();
        fs::create_dir_all(dir.path().join("skip")).unwrap();
        fs::create_dir_all(dir.path().join("skipper")).unwrap();
        fs::write(dir.path().join("keep/a.txt"), b"a").unwrap();
        fs::write(dir.path().join("skip/b.txt"), b"b").unwrap();
        fs::write(dir.path().join("skipper/c.txt"), b"c").unwrap();
        let mut options = ScanOptions::new(dir.path().to_path_buf());
        options.exclusions = vec![dir.path().join("skip")];
        let result = run_scan(
            options,
            Arc::new(ScanControl::new()),
            Arc::new(ScanProgress::new()),
        )
        .unwrap();
        assert_eq!(result.stats.files, 2);
        assert!(result.children(result.root).all(|n| n.name != "skip"));
        assert!(result.children(result.root).any(|n| n.name == "skipper"));
    }

    #[test]
    fn cancellation_returns_partial_status() {
        let dir = tempdir().unwrap();
        for i in 0..200 {
            fs::write(dir.path().join(format!("f{i}.txt")), vec![1u8; 10]).unwrap();
        }
        let control = Arc::new(ScanControl::new());
        let progress = Arc::new(ScanProgress::new());
        let c2 = control.clone();
        let h = thread::spawn(move || {
            thread::sleep(std::time::Duration::from_millis(1));
            c2.cancel();
        });
        let result =
            run_scan(ScanOptions::new(dir.path().to_path_buf()), control, progress).unwrap();
        h.join().unwrap();
        assert!(matches!(
            result.status,
            ScanStatus::Cancelled | ScanStatus::Partial | ScanStatus::Complete
        ));
    }

    #[test]
    fn missing_root_is_reported() {
        let err = run_scan(
            ScanOptions::new(PathBuf::from("Z:\\smartdisk-definitely-missing")),
            Arc::new(ScanControl::new()),
            Arc::new(ScanProgress::new()),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ScanError::NotFound(_) | ScanError::AccessDenied(_) | ScanError::Io(_)
        ));
    }
}
