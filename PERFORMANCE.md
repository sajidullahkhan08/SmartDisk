# Performance

This document records measured performance for SmartDisk 1.0.2. Scanner and duplicate numbers were re-taken after the 1.0.2 performance work; application measurements were also re-taken from the 1.0.2 build. Numbers are from the machine described below, using a release build. Nothing here is estimated unless explicitly labelled.

## Test environment

Measured on 2026-11-11 (scanner and duplicates) and 2026-11-11 (application):

- OS: Microsoft Windows 11 Pro for Workstations, build 22631 (x64)
- CPU: Intel Xeon E3-1545M v5 @ 2.90 GHz, 8 logical processors (4 cores)
- RAM: 15.9 GB
- Storage: the project and benchmark fixtures are on the same local NTFS volume (D:)
- Build: Rust 1.99.1 MSVC x64, `--release` (LTO, one codegen unit)
- UI renderer for automation: `SLINT_BACKEND=winit-software` (software rendering)

## Methodology

- **Scanner benchmark** (`tests/performance_tests.rs`, opt-in): each scope is created under `.testdata/` on the same NTFS volume as the project, scanned once as a warm-up (results discarded) and then three times. The median wall time, process CPU time and working-set samples are reported. The process is started fresh for the benchmark run but all scopes share it; working-set "before/after" values show the effect of each scope, while peak working set is cumulative across the run.
- **Duplicate benchmark** (`tests/performance_tests.rs`, opt-in): 300 files / 230,686,720 bytes forming 80 duplicate pairs of 1 MiB, 100 same-size 512 KiB files that differ in the first byte, and 20 files of 256 KiB with a hard link each. The same in-memory scan result is passed to (a) the 1.0.1 sequential full-file SHA-256 implementation with a 1 MiB buffer per file and (b) the 1.0.2 parallel two-stage implementation. Median of three runs each; both must report the same number of content groups.
- **Files/s** is `files / scan seconds` and describes metadata throughput. It is not a disk-bandwidth number.
- **Treemap** is the median of 20 `squarify` layouts over the direct children of the scanned root.
- **Search** is the median of 10 in-memory queries (`f1`, limit 2000) over the same result.
- **Classification** times 10,000 detailed classifications (path context + rule + evidence string construction), which is heavier than the scan-time classifier.
- **Database size** is the SQLite database plus its WAL/SHM files after recording one scan and vacuuming. Only aggregate history is stored, so it does not grow with per-file scan size.
- **Application measurements** (`tools/measure_app.ps1`): startup is the interval from process creation until a visible, titled main window larger than 400×300 px is responsive; idle CPU is sampled over 8 seconds; UI scan numbers come from the same script with a fixture path pasted into the header. The software renderer (`SLINT_BACKEND=winit-software`) is used for reproducibility of automation; memory with the default GPU renderer may differ.
- **Installed size** is the sum of files installed by the per-user installer, measured after installation.

## Scanner and analysis results

| Scope | Dirs | Files | Logical bytes | Scan ms (median) | Files/s | CPU ms | Working set before → after (MB) | Peak WS (MB) | Treemap ms | Search ms | Classify 10k ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| small | 10 | 200 | 102,400 | 2 | 88,390 | <1 | 4.7 → 4.8 | 5.3 | 0.002 | 0.126 | 0.4 |
| medium | 100 | 10,000 | 2,560,000 | 8 | 1,204,413 | <1 | 8.3 → 9.8 | 11.1 | 0.010 | 1.152 | 21.1 |
| deep (250-level chain) | 250 | 1,000 | 64,000 | 57 | 17,700 | 16 | 8.4 → 8.6 | 11.1 | 0.000 | 0.056 | 44.3 |
| many-small | 40 | 30,000 | 0 | 12 | 2,467,369 | 16 | 12.8 → 16.7 | 18.9 | 0.000 | 5.942 | 20.4 |
| large | 500 | 50,000 | 6,400,000 | 34 | 1,457,743 | 31 | 15.8 → 21.9 | 24.4 | 0.045 | 5.637 | 21.1 |
| very-large | 1,000 | 100,000 | 6,400,000 | 71 | 1,408,693 | 47 | 23.3 → 35.6 | 39.9 | 0.093 | 12.095 | 21.2 |

### Before and after 1.0.2

The 1.0.1 column is the same benchmark on the same machine and fixtures before the directory-enumeration changes. A repeat of the 1.0.2 run was used to reject one noisy sample (an intermediate run measured `large` at 52 ms while unrelated search/tree timings were also about 60% slower, indicating machine load).

| Scope | 1.0.1 scan ms | 1.0.2 scan ms | Change |
|---|---:|---:|---:|
| small | 3 | 2 | −33% |
| medium | 11 | 8 | −27% |
| deep | 80 | 57 | −29% |
| many-small | 19 | 12 | −37% |
| large | 42 | 34 | −19% |
| very-large | 84 | 71 | −15% |

What changed: `FindFirstFileExW` with `FindExInfoBasic` and `FIND_FIRST_EX_LARGE_FETCH` replaces `std::fs::read_dir`; short (8.3) name generation is skipped and entries arrive in batches, so the scanner no longer pays per-entry metadata syscalls. Exclusions no longer force a `PathBuf` join per entry when none are configured, and classification uses stack buffers instead of allocating lowercase copies of every filename.

Notes:

- CPU values below one 15.6 ms process-time tick are reported as `<1` ms; scopes that finish within a single tick cannot be distinguished from zero by `GetProcessTimes`.
- The deep scope is deliberately path-heavy (250 nested levels with few files), which is why its throughput is much lower than the file-count scopes.
- The 100,000-file result is a synthetic fixture on a warm cache. A real system volume will include antivirus, junctions and permission errors, and will usually be slower.
- SQLite database size after recording the 100,000-file scan and vacuuming: 136.6 KiB. History stores one row per scan, not per file.
- The scanner still reads directory metadata rather than the NTFS MFT. More speed on very large NTFS volumes would require a privileged MFT reader, which is outside the current no-admin design.

## Duplicate detection results

| Files | Logical bytes | Content groups | Sequential full hash ms | Optimized ms | Speedup | Bytes read (optimized) |
|---:|---:|---:|---:|---:|---:|---:|
| 300 | 230,686,720 | 100 | 1,303 | 577 | 2.3x | 207,093,760 (89.8% of logical) |

The fixture is deliberately duplicate-heavy, so most 1 MiB files still need a full read in both implementations. The gain comes from hashing up to eight files concurrently and from reusing one buffer per worker instead of allocating 1 MiB per file. In the same fixture, the 100 same-size lookalike files and the 40 hard-linked paths only require the 128 KiB prefilter; hard links are confirmed by file identity and are never read again. Files that share a size but differ in the head or tail of the file are now rejected without reading their middle. Progress counts bytes actually read, and cancellation still returns no partial results.

A synthetic duplicate workload is not the same as a real volume: SHA-256 throughput and the number of size collisions dominate. On a volume where most same-size files are not duplicates, the prefilter avoids a much larger share of reads.

## Application measurements

Measured with `tools/measure_app.ps1` against `target\release\smartdisk.exe` on 2026-11-11. The UI scan fixture was `D:\sdbench` (200 folders × 100 files of 128 bytes, 20,000 files, 2.44 MB logical).

| Metric | Measured |
|---|---:|
| Startup to responsive 1400×880 main window, warm (repeat launch) | 230–233 ms |
| Startup to responsive main window, first launch after rebuild | 1007 ms |
| Startup to responsive main window, installed 1.0.2 build | 266 ms warm (911 ms first launch after install) |
| Idle working set (8 s after startup) | 24.8 MB |
| Idle private memory | 8.9 MB |
| Idle CPU over 8 s | 0% of one logical core |
| UI scan: files / folders | 20,000 / 200 |
| UI scan: scan time reported by the app | 26 ms warm (100 ms on the first scan of a freshly created fixture) |
| UI scan: wall time until the script observed 2 s of idle sampling | 3.17–3.19 s |
| UI scan: process CPU consumed | 31–47 ms |
| UI scan: peak utilization (250 ms sampling) | 6–19% of one core |
| UI scan: peak working set | 36.3–36.5 MB |
| Database on disk (db + WAL + SHM) after the scan | 68 KiB |

The wall time includes the script's mandatory 3-second settle window after pressing Enter; the app's own counter (26 ms warm) and at most 47 ms of process CPU are the meaningful scan figures. The first launch after a rebuild took 1007 ms, consistent with on-access scanning of a new 22 MB executable; repeat launches settled at 230–233 ms. Startup waits for a visible, titled window larger than 400×300 px, so hidden Win32/accessibility helper windows are not mistaken for the UI. A screenshot of the completed scan is at `docs/evidence/perf/ui-scan.png`. Startup and memory use the software renderer; the installed build under the default GPU renderer used about 98 MB working set after launch in 1.0.1, but scan behaviour under that renderer was not measured separately.

## Release artifacts

| Artifact | Size |
|---|---:|
| `release/SmartDisk-Setup.exe` | ≈ 8.3 MiB |
| `release/SmartDisk.exe` | 22,493,696 bytes (21.5 MiB) |
| Installed application directory, including documentation and the uninstaller | 25.7 MiB (13 files) |
| SQLite database after first use (excluding WAL) | 36,864 bytes |

The executable links the C runtime statically (`+crt-static`), so it does not require the Visual C++ Redistributable. Its DLL imports are limited to Windows system libraries (verified with `dumpbin /dependents`); see `tools/listdeps.cmd`.

## Interpretation and limits

- Results are specific to this hardware, filesystem, antivirus configuration and Windows build. They are not a guarantee for other machines.
- Scopes contain ordinary files with small contents. Metadata cost dominates the scan; a volume with millions of files will take longer, while large sequential files do not make the scanner read their contents.
- Antivirus real-time scanning can substantially slow first-time metadata access; a warm scan may benefit from the file-system cache.
- The process working set includes the benchmark harness and SQLite, not only the scanner. The UI measurement is a better indicator of the end-user application.
- Allocated sizes are estimates using a fixed 4096-byte cluster; see KNOWN_LIMITATIONS.md.

## Reproducing

```bat
tools\build.cmd test --release --test performance_tests -- --ignored --nocapture --test-threads=1
powershell -NoProfile -ExecutionPolicy Bypass -File tools\measure_app.ps1 -ScanPath D:\sdbench
```

The benchmark run writes `target/perf/scan-benchmark.md` and `target/perf/duplicate-benchmark.md`; the application script writes `target/perf/app-measurements.md`. The recorded copies from the 1.0.2 measurement session are committed under `docs/evidence/perf/`. All create or read disposable fixtures only. To run only the duplicate benchmark, filter it by name: `tools\build.cmd test --release --test performance_tests benchmark_duplicate_detection -- --ignored --nocapture --test-threads=1`.
