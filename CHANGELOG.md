# Changelog

All notable changes to SmartDisk are documented here. The format follows Keep a Changelog and the project uses semantic versioning.

## [1.0.3] — 2026-11-12

UI polish and documentation release. No functional, scanner or risk-engine changes.

### Changed

- The header drive picker and path field are merged into a single unified location bar, so the address is no longer shown in two separate boxes. The drive dropdown now shows compact labels (`C:`) and the full drive name appears in the path.
- Readability pass across the interface: larger stat values, clearer section titles, slightly taller buttons and navigation items, and more generous spacing in empty states.
- The empty-state action button is wider so labels such as "Scan selected location" are no longer truncated.

### Documentation

- The README was rewritten with badges, screenshots, a download section and a project map.
- Added STORY.md: the motivation and methodology behind SmartDisk — what disk analyzers like WizTree do not tell you (what a file is, whether deleting it is safe) and how SmartDisk answers those questions.
- Added screenshots under `docs/screenshots/`.
- Added `.gitignore` so build artifacts and editor files stay out of the repository.

### Verification

- Test suite re-run on the 1.0.3 build: 128 tests passed, 0 failed.
- UI changes verified with fresh screenshots in both windowed and maximized states.

## [1.0.2] — 2026-11-11

Performance release. Scanning and duplicate detection were optimized; no change to what is scanned or which files are reported as duplicates.

### Changed

- Directory enumeration now uses `FindFirstFileExW` with basic info and large fetch, which skips short-name lookups and batches directory entries. The scan hot path also avoids per-entry `PathBuf` allocations when no exclusions are configured, and classification no longer allocates lowercase copies of every name.
- Duplicate detection hashes candidates in parallel on a bounded worker pool with one reusable buffer per worker instead of allocating a 1 MiB buffer per file.
- Duplicate detection now runs a two-stage content check: a head/tail prefilter (64 KiB each, or the whole file up to 128 KiB) rules out most same-size candidates, and full SHA-256 is only computed for prefilter collisions. Hard links to the same file are confirmed by file identity without reading them again.
- Duplicate progress reports the number of files and bytes actually read; cancellation still discards partial results.

### Measured

- Scanner: 100,000 files 84 ms → 71 ms; 50,000 files 42 ms → 34 ms; 30,000 small files 19 ms → 12 ms; 250-level deep chain 80 ms → 57 ms (release build, same fixtures and machine; medians).
- Duplicate detection: 230 MB fixture (80 duplicate pairs of 1 MiB, 100 same-size non-duplicates, 20 hard-link pairs) 1.30 s → 0.58 s (2.3x) with identical group results.

See PERFORMANCE.md for the full tables and methodology.

## [1.0.1] — 2026-11-08

UI polish release. No functional or scanner changes.

### Fixed

- The maximize button was disabled because the window used a fixed size; the window is now resizable with sensible minimum dimensions and maximizes/restores correctly.
- Header controls (drive list, path field, buttons) were stretched to the full header height and sat flush against the top edge; they are now consistently sized and vertically centered with balanced padding (verified in both windowed and maximized states).
- The empty-state icon rendered as a "missing glyph" box; it now uses the Segoe UI Symbol font like the other icons.
- Two theme switchers existed (header button and Settings checkbox); the header button was removed — theme switching lives in Settings only.
- Font sizes across the UI were too small to read comfortably; all text was enlarged (default 14 px, minimum 11 px).

## [1.0.0] — 2026-11-08

First release.

### Added

- Native Rust + Slint Windows disk analyzer with a dark/light interface and ten pages.
- Metadata-only bounded-parallel scanner with pause, cancel, exclusions, reparse-point detection, unreadable-subtree reporting and exact bottom-up aggregation.
- Overview dashboard: totals, allocated size, counts, errors, elapsed time, volume usage, largest file/folder, category share and top-level folders.
- Explorer-style tree and details with sorting, opening, reveal, properties, copy/cut/paste, rename, Recycle Bin delete and separately confirmed permanent delete.
- Interactive squarified treemap with breadcrumbs, zoom and selection synchronization.
- Largest files and file-types views.
- Offline explainable classification with evidence and confidence; unknown stays unknown.
- Independent deletion-risk engine with protected-location policy, adversarial path handling and conservative cleanup levels.
- Cleanup assistant with grouped recommendations, item-level preview, recovery estimate, Recycle Bin default, opt-in permanent mode and second confirmation.
- Global search by name/path, category, risk, kind, size, extension and age.
- Optional SHA-256 duplicate detection with size pre-grouping, hard-link awareness and cancellation.
- Local SQLite history, recent locations, settings, exclusions and theme persistence.
- Windows resources: application icon and version information.
- Per-user Inno Setup installer with uninstaller, Start menu entry and optional desktop shortcut.
- Test suite covering scanner, classification, risk, aggregation, file operations, and an end-to-end workflow; opt-in performance benchmarks.
- Documentation: README, architecture, user guide, security, performance, testing, known limitations and release notes.

### Known issues

- Binaries are unsigned; Windows SmartScreen may warn on first launch.
- The scanner reads directory metadata rather than the NTFS MFT, so very large NTFS volumes scan slower than specialized tools.
- Some native shell dialogs (conflict prompts, shell progress) are not automated by the UI test helpers.
- Core UI flows are verified manually with screenshots; a full automated UI test harness is not included.
- See [KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md) for the complete list.
