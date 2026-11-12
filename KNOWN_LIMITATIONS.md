# Known limitations

This page lists what SmartDisk does not do, what is approximate, and what was not verified. It is intentionally conservative: absence of an item from this list is not a claim of perfection.

## Scanner and measurements

- **No MFT/USN fast path.** SmartDisk enumerates directories with `read_dir` + metadata. It works on NTFS, exFAT/FAT and shares without elevation, but very large NTFS volumes scan slower than tools that parse the MFT.
- **Allocated size is estimated with a fixed 4096-byte cluster.** Actual cluster size can differ (for example 512 bytes on some volumes, 64 KB on others), and sparse, compressed or deduplicated files are not modelled. Logical size is exact; allocated size is a close estimate.
- **Hard links are counted per path.** Total size and file counts include each hard-linked name. Duplicate detection is hard-link-aware, but the scanner totals are not adjusted for them.
- **Snapshots, not live state.** Files may be created, changed or deleted during or after a scan. Results reflect the moment each directory was read.
- **Reparse points are skipped by default.** Junctions and symlinks are counted as items but their targets are excluded. Enabling "follow reparse points" can include other volumes and is cycle-checked, but totals then depend on link topology.
- **Permission behaviour depends on the account.** Without elevation, some system folders are unreadable and reported as errors. When run with backup privileges, ACLs may be bypassed by the filesystem, so the automated permission-denied test self-skips in such environments.
- **Network and removable media** are scanned if the path is accessible, but latency and disconnect behaviour were not benchmarked. Removable media can disappear mid-scan; errors are reported.

## Classification and risk

- Classification is heuristic. It is explainable and conservative, not authoritative. Renamed files, custom folder layouts and unusual applications can be misclassified.
- Risk levels describe deletion risk based on metadata and policy. They cannot know whether a specific file is the only copy of something important.
- Protected-location matching relies on standard Windows paths. A relocated Windows installation or an unusual mount layout may not be recognized as protected.
- Risk/recommendations do not inspect file contents (except optional duplicate hashing), so a `.tmp` file that is actually valuable is still classified from its name/location/age.
- The cleanup "reclaimable" number is an estimate, not a promise. Files in use, cluster rounding, hard links and regenerated data change the real outcome.

## File operations

- Copy, move and rename use Windows' native shell dialogs. Conflict prompts and shell progress UI are **not** driven by the automated tests; only cancellation-free API behaviour is tested.
- A shell batch failure marks queued items as failed rather than retrying. Partial success is reported per item.
- Permanent deletion cannot be undone.
- Operations are not transactional across multiple items.
- Validation and the actual operation are separate steps; a file can change between them (TOCTOU).
- Paths containing a double quote are refused by the reveal-in-Explorer helper.
- The shell may show its own error dialogs depending on Windows configuration; SmartDisk also reports failed items.

## Duplicates

- Hashing reads entire files and can be slow on large data sets or slow disks. The default minimum size is 1 MB.
- Duplicate identity is size + SHA-256. A malicious SHA-256 collision is theoretical but possible; file identity is used for hard links, not for equivalence checks.
- Files that change during hashing may be reported inconsistently; per-file open/read errors are counted, and cancellation discards partial results.
- Automatic deletion of "extra" copies is intentionally not provided. Each file is acted on explicitly.

## UI and workflow

- The UI is English-only and the visual design is tuned for the default 1400×880 window. High-DPI scaling, very small windows and screen-reader accessibility were not exhaustively tested.
- Core flows (launch, scan, browse, cleanup preview/confirmation/execution, permanent toggle) were verified manually with screenshots. The project does not include a full automated UI test harness.
- Some controls require a mouse; the main keyboard shortcuts are documented in the user guide.
- The treemap is normalized by logical size and does not visualize allocated size, file counts or changes over time.
- Search returns a bounded number of results in the UI; it is not a content-indexed search. File contents are never searched except duplicate hashing.
- Scan history stores aggregate statistics only, not per-file results. Very large history can be cleared in Settings.
- "Remember last location" and window size persist, but there is no multi-window or multi-scan support.
- No portable "settings next to the exe" mode; data is always under `%APPDATA%\SmartDisk`.
- No automatic update mechanism or crash reporting.

## Installer and distribution

- The installer is per-user (`%LOCALAPPDATA%\Programs\SmartDisk`). A per-machine/all-users installation is not provided.
- The executable and installer are **unsigned**. Windows SmartScreen or enterprise policy may block or warn. No Authenticode signature, timestamping or reputation is available in this environment.
- The installer has a single UI language (English), a license page, and no optional components beyond the desktop shortcut. Only silent installation was exercised; the wizard UI and standard (non-elevated) accounts were not tested.
- Windows 10 is allowed by the manifest and installer minimum version, but only Windows 11 build 22631 was tested.
- The executable links the C runtime statically and needs no Visual C++ Redistributable. It does require the standard Windows system DLLs present on Windows 10/11.
- Uninstalling intentionally leaves `%APPDATA%\SmartDisk` data behind.
- The release artifacts were built with the local MSVC toolchain described in PERFORMANCE.md; reproducibility on other machines depends on the same Rust version and dependencies pinned in `Cargo.lock`.

## Verification status

Items not verified in this environment are marked **Not Verified** in [TESTING.md](TESTING.md). In particular:

- Automated UI interaction beyond the manually checked flows.
- Installer behaviour on a clean machine without the build toolchain.
- Antivirus/SmartScreen reputation and code-signing behaviour.
- Network-share and removable-media performance.
- Accessibility and high-DPI behaviour.
- Long-duration stability (multi-hour sessions / scans of multi-million-item volumes).
- Permanent-deletion testing against anything other than disposable fixture files.
