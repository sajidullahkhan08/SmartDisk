//! Opt-in performance benchmarks for the scanner and analysis core.
//!
//! These tests create large disposable trees under `.testdata/`, so they are
//! marked `#[ignore]` and never run as part of the normal suite. Run them from
//! a release build:
//!
//! ```text
//! cargo test --release --test performance_tests -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Results are printed and also written to `target/perf/scan-benchmark.md` so
//! PERFORMANCE.md can quote measured numbers instead of estimates.

mod common;

use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{fixture_root, remove_fixture, write_file};
use sha2::{Digest, Sha256};
use smartdisk::analysis::duplicates::{find_duplicates, DuplicateProgress, DuplicateSnapshot};
use smartdisk::analysis::{overview, search, squarify, SearchFilters};
use smartdisk::core::models::ScanResult;
use smartdisk::intelligence::classify_detailed;
use smartdisk::scanner::{run_scan, ScanControl, ScanOptions, ScanProgress};
use smartdisk::storage::Database;

const WARMUPS: usize = 1;
const RUNS: usize = 3;

struct Scope {
    name: &'static str,
    dirs: usize,
    files_per_dir: usize,
    file_bytes: usize,
    depth_chain: bool,
}

const SCOPES: &[Scope] = &[
    Scope {
        name: "small",
        dirs: 10,
        files_per_dir: 20,
        file_bytes: 512,
        depth_chain: false,
    },
    Scope {
        name: "medium",
        dirs: 100,
        files_per_dir: 100,
        file_bytes: 256,
        depth_chain: false,
    },
    Scope {
        name: "deep",
        dirs: 250,
        files_per_dir: 4,
        file_bytes: 64,
        depth_chain: true,
    },
    Scope {
        name: "many-small",
        dirs: 40,
        files_per_dir: 750,
        file_bytes: 0,
        depth_chain: false,
    },
    Scope {
        name: "large",
        dirs: 500,
        files_per_dir: 100,
        file_bytes: 128,
        depth_chain: false,
    },
    Scope {
        name: "very-large",
        dirs: 1_000,
        files_per_dir: 100,
        file_bytes: 64,
        depth_chain: false,
    },
];

#[derive(Clone, Copy, Default)]
struct ProcessSample {
    working_set: u64,
    peak_working_set: u64,
    cpu_100ns: u64,
}

#[cfg(windows)]
fn sample_process() -> ProcessSample {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    let mut counters = PROCESS_MEMORY_COUNTERS::default();
    let mut sample = ProcessSample::default();
    unsafe {
        if K32GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
        .as_bool()
        {
            sample.working_set = counters.WorkingSetSize as u64;
            sample.peak_working_set = counters.PeakWorkingSetSize as u64;
        }
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        if GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
        .is_ok()
        {
            let to_u64 = |value: FILETIME| {
                ((value.dwHighDateTime as u64) << 32) | value.dwLowDateTime as u64
            };
            sample.cpu_100ns = to_u64(kernel) + to_u64(user);
        }
    }
    sample
}

#[cfg(not(windows))]
fn sample_process() -> ProcessSample {
    ProcessSample::default()
}

fn cpu_ms(delta_100ns: u64) -> f64 {
    delta_100ns as f64 / 10_000.0
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 0 {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    } else {
        values[n / 2]
    }
}

fn build_scope(scope: &Scope) -> PathBuf {
    let root = fixture_root(scope.name);
    if scope.depth_chain {
        let mut current = root.clone();
        for level in 0..scope.dirs {
            current = current.join(format!("level{level:04}"));
            for file in 0..scope.files_per_dir {
                write_file(
                    &current.join(format!("f{file}.bin")),
                    scope.file_bytes,
                );
            }
        }
    } else {
        for dir in 0..scope.dirs {
            let folder = root.join(format!("dir{dir:04}"));
            for file in 0..scope.files_per_dir {
                write_file(
                    &folder.join(format!("f{file}.bin")),
                    scope.file_bytes,
                );
            }
        }
    }
    root
}

fn timed_scan(root: &Path) -> (ScanResult, Duration, f64) {
    let before = sample_process();
    let started = Instant::now();
    let result = run_scan(
        ScanOptions::new(root.to_path_buf()),
        Arc::new(ScanControl::new()),
        Arc::new(ScanProgress::new()),
    )
    .expect("benchmark scan should succeed");
    let elapsed = started.elapsed();
    let after = sample_process();
    (result, elapsed, cpu_ms(after.cpu_100ns.saturating_sub(before.cpu_100ns)))
}

fn benchmark_search(result: &ScanResult) -> f64 {
    let filters = SearchFilters {
        query: "f1".into(),
        limit: 2000,
        ..SearchFilters::default()
    };
    let now = smartdisk::core::util::now_unix();
    let mut times = Vec::new();
    for _ in 0..10 {
        let started = Instant::now();
        let hits = search(result, &filters, now);
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        std::hint::black_box(hits);
    }
    median(times)
}

fn benchmark_treemap(result: &ScanResult) -> f64 {
    let sizes: Vec<f64> = result
        .child_ids(result.root)
        .into_iter()
        .filter_map(|id| result.node(id))
        .map(|node| node.size as f64)
        .collect();
    let mut times = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        let rects = squarify(&sizes);
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        std::hint::black_box(rects);
    }
    median(times)
}

fn benchmark_classification(result: &ScanResult) -> f64 {
    let mut checked = 0usize;
    let started = Instant::now();
    for id in 1..result.nodes.len() as u32 {
        let Some(node) = result.node(id) else { continue };
        if node.is_dir {
            continue;
        }
        let classification =
            classify_detailed(Path::new(&result.path_of(id)), &node.name, false, node.attributes);
        std::hint::black_box(classification);
        checked += 1;
        if checked >= 10_000 {
            break;
        }
    }
    started.elapsed().as_secs_f64() * 1000.0
}

fn database_size(result: &ScanResult) -> u64 {
    let root = fixture_root("db-size");
    let path = root.join("perf.db");
    let size = {
        let database = Database::open(&path).expect("open benchmark database");
        database.record_scan(result).expect("record benchmark scan");
        let _ = database.vacuum();
        let mut total = database.size_bytes();
        for suffix in ["-wal", "-shm"] {
            let extra = PathBuf::from(format!("{}{suffix}", path.display()));
            total += fs::metadata(extra).map(|m| m.len()).unwrap_or(0);
        }
        total
    };
    remove_fixture(&root);
    size
}

fn environment() -> String {
    let processor = std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "unknown".into());
    let cores = std::env::var("NUMBER_OF_PROCESSORS").unwrap_or_else(|_| "unknown".into());
    let os_detail = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "(Get-CimInstance Win32_OperatingSystem).Caption + ' build ' + (Get-CimInstance Win32_OperatingSystem).BuildNumber + '; ' + (Get-CimInstance Win32_Processor).Name + '; ' + [math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory/1GB,1) + ' GB RAM'",
        ])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| format!("{} {}", std::env::consts::OS, std::env::consts::ARCH));
    format!("{os_detail}; logical processors: {cores}; {processor}")
}

#[test]
#[ignore = "performance benchmark; creates a large disposable tree"]
fn benchmark_scan_and_analysis_scopes() {
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    let environment = environment();
    println!("environment: {environment}");
    println!("profile: {profile}");

    let mut rows: Vec<String> = Vec::new();
    rows.push(format!(
        "| scope | dirs | files | logical bytes | scan ms (median) | files/s | CPU ms | WS before / after (MB) | peak WS (MB) | treemap ms | search ms | classify 10k ms |"
    ));
    rows.push(
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
            .to_string(),
    );

    let mut db_row = String::new();
    for scope in SCOPES {
        let root = build_scope(scope);
        let mut scan_times = Vec::new();
        let mut cpu_times = Vec::new();
        let mut file_count = 0u64;
        let mut dir_count = 0u64;
        let mut total_bytes = 0u64;
        let mut before_ws = 0u64;
        let mut after_ws = 0u64;
        let mut peak_ws = 0u64;
        let mut last_result: Option<ScanResult> = None;

        for run in 0..(WARMUPS + RUNS) {
            let before = sample_process();
            let (result, elapsed, cpu) = timed_scan(&root);
            if run >= WARMUPS {
                scan_times.push(elapsed.as_secs_f64() * 1000.0);
                cpu_times.push(cpu);
                before_ws = before.working_set;
                after_ws = sample_process().working_set;
                peak_ws = sample_process().peak_working_set;
            }
            if run == WARMUPS {
                file_count = result.stats.files;
                dir_count = result.stats.dirs;
                total_bytes = result.stats.total_size;
            }
            last_result = Some(result);
        }

        let result = last_result.expect("at least one scan");
        let scan_ms = median(scan_times);
        let cpu_median = median(cpu_times);
        let files_per_sec = if scan_ms > 0.0 {
            file_count as f64 / (scan_ms / 1000.0)
        } else {
            0.0
        };
        let treemap_ms = benchmark_treemap(&result);
        let search_ms = benchmark_search(&result);
        let classify_ms = benchmark_classification(&result);

        let overview = overview(&result);
        assert_eq!(overview.files, file_count);
        db_row = format!(
            "database after recording a {}-file scan: {:.1} KiB",
            file_count,
            database_size(&result) as f64 / 1024.0
        );

        println!(
            "scope={} dirs={} files={} bytes={} scan_ms={:.1} files_per_s={:.0} cpu_ms={:.0} ws_before_mb={:.1} ws_after_mb={:.1} peak_ws_mb={:.1} treemap_ms={:.3} search_ms={:.3} classify_10k_ms={:.1}",
            scope.name,
            dir_count,
            file_count,
            total_bytes,
            scan_ms,
            files_per_sec,
            cpu_median,
            before_ws as f64 / 1_048_576.0,
            after_ws as f64 / 1_048_576.0,
            peak_ws as f64 / 1_048_576.0,
            treemap_ms,
            search_ms,
            classify_ms
        );
        rows.push(format!(
            "| {} | {} | {} | {} | {:.0} | {:.0} | {:.0} | {:.1} / {:.1} | {:.1} | {:.3} | {:.3} | {:.1} |",
            scope.name,
            dir_count,
            file_count,
            total_bytes,
            scan_ms,
            files_per_sec,
            cpu_median,
            before_ws as f64 / 1_048_576.0,
            after_ws as f64 / 1_048_576.0,
            peak_ws as f64 / 1_048_576.0,
            treemap_ms,
            search_ms,
            classify_ms
        ));

        drop(result);
        remove_fixture(&root);
    }

    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("perf");
    fs::create_dir_all(&out_dir).expect("create performance output directory");
    let report_path = out_dir.join("scan-benchmark.md");
    let mut report = fs::File::create(&report_path).expect("create report");
    writeln!(report, "# SmartDisk scan benchmark").unwrap();
    writeln!(report).unwrap();
    writeln!(report, "- Environment: {environment}").unwrap();
    writeln!(report, "- Build profile: {profile}").unwrap();
    writeln!(
        report,
        "- Method: {} warm-up scan(s) plus {} measured scans per scope; median reported. Peak working set is process-wide and cumulative.",
        WARMUPS, RUNS
    )
    .unwrap();
    writeln!(report, "- {} ", db_row).unwrap();
    writeln!(report).unwrap();
    writeln!(report, "{}", rows.join("\n")).unwrap();
    println!("report written to {}", report_path.display());
}

/// Duplicate-heavy fixture with three distinct cases:
/// - 80 pairs of 1 MiB files with identical content (full-hash duplicates),
/// - 100 files of 512 KiB that share a size but differ in the first byte
///   (the prefilter must rule them out),
/// - 20 files of 256 KiB with a hard link each (identity short-circuit).
fn build_duplicate_fixture() -> PathBuf {
    let root = fixture_root("duplicates");
    let dups = root.join("dups");
    fs::create_dir_all(&dups).unwrap();
    for i in 0..80usize {
        let data = vec![(i % 250 + 1) as u8; 1024 * 1024];
        fs::write(dups.join(format!("orig-{i:03}.bin")), &data).unwrap();
        fs::write(dups.join(format!("copy-{i:03}.bin")), &data).unwrap();
    }
    let lookalikes = root.join("lookalikes");
    fs::create_dir_all(&lookalikes).unwrap();
    for i in 0..100usize {
        let mut data = vec![0u8; 512 * 1024];
        data[0] = (i + 1) as u8;
        fs::write(lookalikes.join(format!("same-size-{i:03}.bin")), &data).unwrap();
    }
    let links = root.join("hardlinks");
    fs::create_dir_all(&links).unwrap();
    for i in 0..20usize {
        let original = links.join(format!("original-{i:02}.bin"));
        fs::write(&original, vec![(i + 1) as u8; 256 * 1024]).unwrap();
        fs::hard_link(&original, links.join(format!("link-{i:02}.bin"))).unwrap();
    }
    root
}

/// The 1.0.1 algorithm: sequential SHA-256 over the complete contents of every
/// same-size candidate, with a 1 MiB buffer allocated per file. Returns elapsed
/// milliseconds and the number of content groups it would report.
fn sequential_full_hash(result: &ScanResult) -> (f64, usize) {
    let mut by_size: HashMap<u64, Vec<u32>> = HashMap::new();
    for id in 1..result.nodes.len() as u32 {
        let node = &result.nodes[id as usize];
        if node.is_dir {
            continue;
        }
        by_size.entry(node.size).or_default().push(id);
    }
    let started = Instant::now();
    let mut groups = 0usize;
    for ids in by_size.into_values() {
        if ids.len() < 2 {
            continue;
        }
        let mut hashed: HashMap<[u8; 32], usize> = HashMap::new();
        for id in ids {
            let path = result.path_of(id);
            let Ok(mut file) = File::open(&path) else {
                continue;
            };
            let mut hasher = Sha256::new();
            let mut buffer = vec![0u8; 1024 * 1024];
            loop {
                match file.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => hasher.update(&buffer[..n]),
                    Err(_) => break,
                }
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&hasher.finalize());
            *hashed.entry(key).or_default() += 1;
        }
        groups += hashed.values().filter(|count| **count >= 2).count();
    }
    (started.elapsed().as_secs_f64() * 1000.0, groups)
}

fn optimized_duplicates(result: &ScanResult) -> (f64, usize, DuplicateSnapshot) {
    let progress = Arc::new(DuplicateProgress::new());
    let started = Instant::now();
    let groups = find_duplicates(result, 1, progress.clone());
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    (elapsed, groups.len(), progress.snapshot())
}

#[test]
#[ignore = "performance benchmark; creates a large disposable tree"]
fn benchmark_duplicate_detection() {
    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    let environment = environment();
    let root = build_duplicate_fixture();
    let result = run_scan(
        ScanOptions::new(root.clone()),
        Arc::new(ScanControl::new()),
        Arc::new(ScanProgress::new()),
    )
    .expect("benchmark scan should succeed");

    let mut baseline_times = Vec::new();
    let mut optimized_times = Vec::new();
    let mut baseline_groups = 0usize;
    let mut optimized_groups = 0usize;
    let mut snapshot = DuplicateSnapshot::default();
    for _ in 0..3 {
        let (ms, groups) = sequential_full_hash(&result);
        baseline_times.push(ms);
        baseline_groups = groups;
    }
    for _ in 0..3 {
        let (ms, groups, snap) = optimized_duplicates(&result);
        optimized_times.push(ms);
        optimized_groups = groups;
        snapshot = snap;
    }
    assert_eq!(
        baseline_groups, optimized_groups,
        "optimized hashing must find exactly the same content groups"
    );

    let baseline_ms = median(baseline_times);
    let optimized_ms = median(optimized_times);
    let speedup = if optimized_ms > 0.0 {
        baseline_ms / optimized_ms
    } else {
        0.0
    };
    let files = result.stats.files;
    let bytes = result.stats.total_size;
    println!(
        "duplicates files={} bytes={} groups={} baseline_ms={:.1} optimized_ms={:.1} speedup={:.1}x hashed_bytes={} errors={}",
        files,
        bytes,
        optimized_groups,
        baseline_ms,
        optimized_ms,
        speedup,
        snapshot.bytes_hashed,
        snapshot.errors
    );

    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("perf");
    fs::create_dir_all(&out_dir).expect("create performance output directory");
    let report_path = out_dir.join("duplicate-benchmark.md");
    let mut report = fs::File::create(&report_path).expect("create report");
    writeln!(report, "# SmartDisk duplicate detection benchmark").unwrap();
    writeln!(report).unwrap();
    writeln!(report, "- Environment: {environment}").unwrap();
    writeln!(report, "- Build profile: {profile}").unwrap();
    writeln!(
        report,
        "- Fixture: 160 files of 1 MiB forming 80 duplicate pairs; 100 files of 512 KiB that share a size but differ in the first byte; 20 files of 256 KiB each with a hard link (40 paths)."
    )
    .unwrap();
    writeln!(
        report,
        "- Baseline: the 1.0.1 algorithm - sequential full-file SHA-256 for every same-size candidate, 1 MiB buffer allocated per file."
    )
    .unwrap();
    writeln!(
        report,
        "- Optimized: parallel two-stage hashing (head/tail prefilter, full hash only for prefilter collisions) with hard-link identity short-circuit. Each worker reuses one buffer."
    )
    .unwrap();
    writeln!(
        report,
        "- Method: one in-memory scan result; median of 3 runs for each implementation on the same machine."
    )
    .unwrap();
    writeln!(report).unwrap();
    writeln!(
        report,
        "| files | logical bytes | content groups | sequential full hash ms | optimized ms | speedup | bytes read (optimized) |"
    )
    .unwrap();
    writeln!(report, "|---:|---:|---:|---:|---:|---:|---:|").unwrap();
    writeln!(
        report,
        "| {} | {} | {} | {:.0} | {:.0} | {:.1}x | {} ({:.1}% of logical) |",
        files,
        bytes,
        optimized_groups,
        baseline_ms,
        optimized_ms,
        speedup,
        snapshot.bytes_hashed,
        if bytes > 0 {
            snapshot.bytes_hashed as f64 * 100.0 / bytes as f64
        } else {
            0.0
        }
    )
    .unwrap();
    writeln!(report).unwrap();
    writeln!(
        report,
        "- Files hashed (prefilter pass): {}; errors: {}.",
        snapshot.files_hashed, snapshot.errors
    )
    .unwrap();
    println!("report written to {}", report_path.display());

    drop(result);
    remove_fixture(&root);
}
