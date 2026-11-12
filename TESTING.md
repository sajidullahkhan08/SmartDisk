# Testing and verification

This document records what was actually executed for SmartDisk, on which environment, and what remains unverified. Results are not extrapolated. The bulk of the log covers 1.0.2; a 1.0.3 re-verification section near the top records what was re-executed for the UI polish release.

## Environment

- Microsoft Windows 11 Pro for Workstations, build 22631, x64
- Intel Xeon E3-1545M v5 @ 2.90 GHz, 8 logical processors, 15.9 GB RAM
- Rust 1.99.1 (MSVC x64), `cargo` test profile and `--release` profile
- Slint 1.18.1, Inno Setup 6.7.3
- The test session ran elevated and had `SeBackupPrivilege`, `SeRestorePrivilege` and `SeDebugPrivilege` enabled. This affects the permission-denied test, as described below.

## How to run

```bat
tools\build.cmd test
tools\build.cmd test --release --test performance_tests -- --ignored --nocapture --test-threads=1
powershell -NoProfile -ExecutionPolicy Bypass -File tools\measure_app.ps1 -ScanPath D:\sdbench
```

`tools\build.cmd` initializes `vcvars64.bat` and forwards the remaining arguments to Cargo. The normal suite is self-contained: fixtures live under `.testdata\` in the repository and cleanup tests use dedicated temporary trees. The performance benchmark is opt-in because it creates a large disposable tree.

## Final suite result

Command: `tools\build.cmd test` (debug profile) and `tools\build.cmd test --release`, run after the last functional source changes in 1.0.2.

| Target | Tests | Result |
|---|---:|---|
| `src/lib.rs` unit tests | 65 | 65 passed |
| `tests/aggregation_tests.rs` | 11 | 11 passed |
| `tests/classification_tests.rs` | 8 | 8 passed |
| `tests/file_operation_tests.rs` | 13 | 13 passed |
| `tests/integration_tests.rs` | 1 | 1 passed |
| `tests/risk_tests.rs` | 15 | 15 passed |
| `tests/scanner_tests.rs` | 15 | 15 passed |
| `tests/performance_tests.rs` | 2 | ignored by default (opt-in benchmarks) |
| **Total executed** | **128** | **128 passed, 0 failed** |

Doc-tests: none. The suite executes in a few seconds after compilation.

## 1.0.3 re-verification

The 1.0.3 release changed only the Slint UI (unified location bar, component sizing, readability) and documentation; no Rust logic changed. The following was re-executed on the 1.0.3 sources on 2026-11-12, same environment as above (Visual Studio 2022 Build Tools was reinstalled before the build; the MSVC toolchain was otherwise unchanged):

- `tools\build.cmd test` (debug): 128 passed, 0 failed (same targets as the table above).
- `tools\build.cmd test --release`: 128 passed, 0 failed.
- `cargo check` clean; release build produced `SmartDisk.exe` (22,523,392 bytes) with version resource 1.0.3.0.
- UI verified visually on the release build with the software renderer (`tools\shoot_pages.ps1`, `tools\shoot_empty.ps1`, `tools\shoot_dark.ps1`): the unified location bar renders correctly in both light and dark themes (`docs/screenshots/ui-overview.png`, `docs/screenshots/ui-dark.png`), the empty-state action button shows its full "Scan selected location" label (`docs/screenshots/ui-empty.png`), and Explorer/Treemap render normally (`docs/screenshots/ui-explorer.png`, `docs/screenshots/ui-treemap.png`). The dark-theme capture toggles the setting and restores it afterwards.
- Installer recompiled with Inno Setup 6.7.3 for version 1.0.3. Silent per-user install exited 0 (13 files, 25.7 MiB, version resource 1.0.3.0); the installed executable launched and showed a responsive main window; silent uninstall exited 0, removed the application directory and preserved `%APPDATA%\SmartDisk\smartdisk.db`.
- `release\checksums.txt` regenerated for the 1.0.3 binaries.

## Coverage by area

### Scanner (`tests/scanner_tests.rs`, unit tests)

Nested aggregation; zero-byte files; hidden and read-only attributes; unicode and long names; duplicate names in different folders; absolute and stable paths; deep nesting beyond 250 levels without recursion overflow; junction detection and non-traversal; component-aware exclusions; worker-count independence; cancellation with partial results; missing roots and file roots; allocated-size rounding; permission-denied subtree handling (see caveat below).

### Classification (`tests/classification_tests.rs`, unit tests)

Every documented category example; case-insensitive matching; folder context (a file is classified against its containing folder, as the scanner does); Windows locations taking precedence over extension alone; installers, archives and database files; unknown results carrying low confidence; detailed explanations containing a rule and evidence.

### Risk (`tests/risk_tests.rs`, unit tests)

Drive/profile/Windows/Program Files roots protected; files directly inside protected roots protected; parent-traversal (`..`) cannot hide a protected path; similar names such as `Windows.old2` or `Program Files Backup` not confused with protected roots; unicode profile paths; system attributes; reparse points never reducing risk; unknown remaining unknown and cleanup-ineligible; classification alone not deciding safety; detailed reports with evidence and consequences; cleanup levels never assigned to protected locations.

### Aggregation and analysis (`tests/aggregation_tests.rs`, unit tests)

Overview totals and top-level shares; parent sums consistent with children; category and extension totals adding up; search by name, path, extension, category and size; treemap area proportionality; percentage arithmetic without division by zero; cleanup plan de-duplication and blocking of critical nodes; conservative recommendations; duplicate detection by content (not names); hard links not counted as recoverable copies; cancellation producing no partial result; large files with identical head/tail but different middle content not treated as duplicates; large identical files found.

### File operations (`tests/file_operation_tests.rs`, unit tests)

Copy and move of file and folder trees; multi-item copy reporting per item; partial failure reporting; rename updating the path; invalid/colliding rename rejection; protection of protected paths from rename; copy/move unique-target collision handling; folder cannot be moved inside itself; missing-source validation; Recycle Bin rejecting missing items before any change; a recycle operation removing the source and being reversible in principle; permanent deletion kept separate and tested only on a dedicated fixture tree.

### End-to-end (`tests/integration_tests.rs`)

Scan a fixture, inspect child nodes and paths, search, build a cleanup recommendation and plan, perform a copy/recycle-style operation inside the fixture, rescan, and verify the changed totals. No system directory is touched.

### Performance (`tests/performance_tests.rs`, opt-in)

Two ignored benchmarks. The scanner benchmark covers six scopes (small, medium, deep, many-small, large, very-large), one warm-up and three measured runs each, with median scan time, throughput, process CPU, working set, treemap generation, search latency and detailed-classification time. The duplicate benchmark creates 300 files / 230,686,720 bytes (80 duplicate pairs of 1 MiB, 100 same-size non-duplicates, 20 hard-link pairs) and compares the 1.0.1 sequential full-hash algorithm against the 1.0.2 parallel two-stage implementation on the same in-memory scan result; the two must report the same number of content groups. Results are recorded in [PERFORMANCE.md](PERFORMANCE.md); the harness writes them to `target/perf/` at run time and the recorded copies are committed under `docs/evidence/perf/scan-benchmark.md` and `docs/evidence/perf/duplicate-benchmark.md`.

## UI verification

- The 1.0.1 UI fixes were each verified against the release build:
  - Maximize: the Win32 style of the main window includes `WS_MAXIMIZEBOX`/`WS_THICKFRAME` (style `0x16cf0000` vs `0x16ca0000` before the fix), and programmatic `ShowWindow(SW_MAXIMIZE)` maximized the window. Screenshot: `docs/evidence/perf/ui-maximized.png`.
  - Header layout: header controls are a consistent 34 px high inside a 64 px header, vertically centered with balanced 14 px top/bottom padding (equal padding rather than layout alignment, which had collapsed the path field's horizontal stretch during the fix); verified in windowed and maximized screenshots (`docs/evidence/perf/ui-overview.png`, `docs/evidence/perf/ui-scan.png`, `docs/evidence/perf/ui-maximized.png`).
  - Single theme control: the header theme button was removed; only the Settings "Dark theme" checkbox remains (screenshot `docs/evidence/perf/ui-settings.png`).
  - Font sizes: default text is 14 px and the smallest explicit size is 11 px; Overview, Explorer, treemap and Settings screenshots show no clipped or overlapping text (`docs/evidence/perf/ui-overview.png`, `docs/evidence/perf/ui-explorer.png`, `docs/evidence/perf/ui-treemap.png`, `docs/evidence/perf/ui-settings.png`).
- The release UI was launched with the software renderer (`SLINT_BACKEND=winit-software`) and driven by `tools/measure_app.ps1`: the path was pasted into the header, Enter started the scan, and the script polled CPU/working set until idle. A post-paste screenshot (`docs/evidence/perf/ui-scan-after-paste.png`) confirms the path field received focus and the scan engaged.
- A 20,000-file fixture (`D:\sdbench`, 200 folders × 100 files) completed successfully in the 1.0.2 release build. The screenshot at `docs/evidence/perf/ui-scan.png` shows the Overview page with `Complete` status, 20,000 files, 200 folders, 2.44 MB logical, 78.1 MB estimated allocation, 0 access errors, a populated top-level folder list and the completion toast. The app-reported scan time was 26 ms warm (100 ms on the first scan of the freshly created fixture); previous 1.0.1 runs reported 26–30 ms, so the UI scan path is unchanged or slightly faster, and the benchmark numbers above show the improvement at larger file counts.
- The script requires a visible, titled main window larger than 400×300 px before recording startup, so hidden helper windows created during initialization are not counted as the UI.
- Startup, idle CPU/memory, UI scan memory and database size are in [PERFORMANCE.md](PERFORMANCE.md).
- The installed build was launched separately and produced a responsive main window (911 ms first launch, 266 ms repeat) and completed a scan. Measurements in 1.0.2 used the software renderer for reproducibility; in 1.0.1 the installed build under the default GPU renderer used about 98 MB working set after startup, and that renderer was not re-measured.

## Installer verification

- `release\SmartDisk-Setup.exe` was compiled with Inno Setup 6.7.3 from `installer\smartdisk.iss` for version 1.0.2.
- Silent per-user installation (`/SILENT /NORESTART`) exited 0. It installed `SmartDisk.exe` (version resource 1.0.2.0), all documentation and the uninstaller under `%LOCALAPPDATA%\Programs\SmartDisk` (13 files, 25.7 MiB).
- The installed executable launched and showed a responsive main window: 911 ms on the first launch after installation and 266 ms on a repeat launch. A UI-driven scan of the 20,000-file fixture completed normally; the bundled `PERFORMANCE.md` was checked to be the 1.0.2 version.
- Silent uninstallation exited 0, removed the application directory, and preserved `%APPDATA%\SmartDisk\smartdisk.db` as designed.
- `release\checksums.txt` contains SHA-256 values for both release binaries. Verify with `certutil -hashfile <file> SHA256`.
- `dumpbin /dependents` shows only Windows system DLLs; the executable links the C runtime statically and does not need the Visual C++ Redistributable.

## Known test limitations

- **Permission-denied scanning is not fully verified in this environment.** The session had backup privileges enabled, so an `icacls` deny ACE did not prevent directory enumeration. `permission_denied_subtree_is_reported_without_aborting_scan` detects that condition and returns early (it is reported as passed), rather than producing a false pass. The scanner's error-reporting path is otherwise covered by the missing-root and cancellation tests, but a true access-denied directory must be verified under a standard, non-backup-privileged account. Status: **Not Verified**.
- **Installer GUI wizard** was not clicked through manually; only the silent path was exercised. The wizard-specific pages (welcome, license, directory, desktop-icon task, finish/launch) are compiled from standard Inno Setup directives. Status: **Not Verified**.
- **Standard-user (non-elevated) install and scan** were not tested under a separate account. The installer is configured as per-user with `PrivilegesRequired=lowest`. Status: **Not Verified**.
- **Windows 10** compatibility is declared by the manifest/minimum version but was not tested; all measurements are from Windows 11 build 22631. Status: **Not Verified**.
- **Code signing / SmartScreen** was not tested; binaries are unsigned. Status: **Not Verified**.
- **High-DPI scaling, screen-reader accessibility and keyboard-only navigation across every page** were not exhaustively tested. Status: **Partially Verified** (keyboard shortcuts exist and basic interaction was automated; no accessibility audit).
- **Real-volume scans** were not run against an entire physical drive. The largest measured scan is 100,000 synthetic files (71 ms median) and the UI scan is 20,000 files. Large real volumes include antivirus overhead, reparse points and permission errors that are not represented by fixtures. Status: **Partially Verified**.
- **Native shell conflict/progress dialogs** for copy/move were not automated; file-operation logic and per-item outcomes are covered by tests, but the Windows dialogs themselves are not. Status: **Not Verified**.
- **Duplicate hashing on real user data** was tested only with fixtures; very large files and files changing during hashing are not covered beyond cancellation/error handling. The benchmark covers 1 MiB duplicates, same-size non-duplicates and hard links, not multi-gigabyte files. Status: **Partially Verified**.
