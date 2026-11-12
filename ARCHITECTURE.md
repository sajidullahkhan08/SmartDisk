# Architecture

SmartDisk is a single native Windows application written in Rust. The core library (`smartdisk`) is independent of the UI and can be tested headlessly; the Slint presentation layer binds to a controller (`ui::backend`) that owns application state.

## Component map

```text
main.rs
  └── app::run
        └── ui::run ────────────── Slint window (ui/*.slint)
              │                       ▲ properties / callbacks
              └── ui::backend ────────┘
                    │
    ┌───────────────┼────────────────┬──────────────────┐
    ▼               ▼                ▼                  ▼
 scanner        analysis         intelligence        cleanup
    │               │                │                  │
    └───────────────┴────────────────┴──────────────────┘
                    │
              core::models (ScanResult, FsNode, categories, risk)
                    │
     ┌──────────────┼──────────────┐
     ▼              ▼              ▼
  storage        platform      file_operations
  SQLite         drives/shell  IFileOperation/trash
```

## Scan pipeline

`scanner::run_scan` is synchronous and is normally called from a coordinator thread in `services::scan_service`. The UI polls progress and completion without blocking.

1. **Input validation.** The root is made absolute, normalized for display, and opened with `symlink_metadata` and `read_dir` so missing paths, files and access-denied roots fail early with an actionable `ScanError`.
2. **Bounded parallel traversal.** A crossbeam bounded channel feeds directory jobs to `N` workers (`ScanOptions::effective_workers`: automatic = CPUs − 1 clamped to 1–8, explicit = 1–32, low-resource mode = at most 2). Workers list entries with `platform::dir_enum::list_directory`, which calls `FindFirstFileExW` using `FindExInfoBasic` and `FIND_FIRST_EX_LARGE_FETCH`: short 8.3 names are not looked up and entries arrive in batches. Each `WIN32_FIND_DATAW` already carries attributes, size and timestamps, so no per-entry metadata call is needed. Exclusions are applied per entry (component-aware) only when configured.
3. **Per-directory context.** Each worker builds one `PathContext` per directory. Children are classified and risk-assessed against that directory context. This is both faster and semantically correct: a file named `data.bin` inside `\Cache\` inherits the cache-folder evidence. The scan-time classifier lowercases names and extensions into stack buffers, so ordinary entries do not allocate for classification.
4. **Aggregation.** The coordinator inserts nodes into a flat arena (`Vec<FsNode>`) with parent/child indices, then finalizes directories bottom-up as their children complete. Sizes, allocated sizes, file counts and directory counts are accumulated exactly once. No recursion is used for aggregation, so deep trees cannot overflow the stack.
5. **Reparse points.** Junctions and symbolic links are reported but not traversed by default. When following is explicitly enabled, canonical paths are tracked to avoid cycles.
6. **Cancellation and pausing.** `ScanControl` is shared by all workers and checked between directories. Cancellation returns partial data with `ScanStatus::Cancelled`; it never reports partial data as complete.
7. **Result.** `ScanResult` contains the node arena, stats, errors (`ScanIssue`), warnings, status, timestamps and optional volume information. The result is immutable once returned.

The scanner reads metadata only. File contents are opened only by the optional duplicate analysis.

## Data model

`core::models` defines:

- `Category` — the offline classification taxonomy.
- `Confidence` — how strong the classification evidence is.
- `RiskLevel` — `Critical`, `High`, `Review`, `Lower`, `Unknown`.
- `FsNode` — flat arena node with name, size, allocated size, timestamps, attributes, classification, risk and child links.
- `ScanResult` — arena plus indexing helpers (`node`, `path_of`, `child_ids`, `depth_of`, `descendants`).

Paths are reconstructed on demand from parent links. This keeps the per-node footprint small for scans with hundreds of thousands of items.

## Classification

`intelligence::classifier` is a deterministic rule engine:

1. Recycle Bin location.
2. Known Windows system files (for example pagefile, hiberfil, NTUSER.DAT).
3. Windows directories (temp, SoftwareDistribution, logs, Prefetch, system).
4. Program Files.
5. ProgramData (cache/log/temp refinements).
6. Browser profiles and browser cache.
7. Per-user AppData and its temp/cache/log areas.
8. Development dependency trees, version-control projects, virtual machines.
9. Cloud-sync, game, backup locations.
10. User shell folders (Pictures, Videos, Music, Downloads, Desktop, Documents).
11. Extension rules.
12. Folder-name fallbacks and finally `Unknown`.

All location matching is component-based, case-insensitive and does not use substring matching. Detailed classification returns the matched rule, evidence lines and a human explanation. Adversarial `..` components are lexically normalized before matching so a path cannot disguise a protected location.

## Risk engine

`intelligence::risk` is independent of the classifier's confidence. `assess` combines:

- protected-location policy (`PathContext::is_critical_location`),
- category,
- item type (file/folder),
- age,
- Windows attributes (system, reparse point).

Rules of note:

- Drive roots, profile roots, the Windows installation (except narrow cleanup zones), Program Files, ProgramData\Microsoft, System Volume Information and boot areas are `Critical`.
- Reparse points are never lowered below `High` because their target is hidden.
- A system-flagged item in a protected location is always `Critical`.
- User data categories are `High` (important), not "recommended for deletion".
- Unknown stays `Unknown`; no cleanup recommendation is made.
- `Review` and `Lower` require recognized temporary/cache/log context and, for files, age thresholds.

`is_protected_path` is the guard used before file operations. It evaluates the path and, for files, the containing directory, so a file placed directly in `C:\` or a profile root is rejected while ordinary folders such as `Documents` are not treated as system areas.

## Cleanup assistant

`cleanup::recommendations` iterates the scan result and offers only files whose risk is cleanup-eligible in recognized locations. Groups are temporary files, caches, logs, Windows Update downloads and Recycle Bin contents. Protected folders are excluded by folder context; folders themselves are never candidates, avoiding double-counting.

`cleanup::build_plan` validates explicit selections: it deduplicates nodes, rejects protected nodes (`blocked`) and unknown-risk nodes (`unknown`), and estimates recoverable bytes. The estimate is explicitly labelled as an estimate because cluster allocation, hard links and future recreation can differ.

The UI shows the plan, lets the user switch between Recycle Bin and permanent deletion, and requires confirmation. Execution calls the file-operation layer; on success it rescans to show the updated state.

## File operations

`file_operations::operations` uses `IFileOperation` on a short-lived STA thread for copy, move and rename. Windows therefore supplies its native conflict and progress UI. Recycle Bin deletion uses the `trash` crate (native shell delete, no UI). Permanent deletion is implemented separately, removes read-only attributes first, and never follows reparse points. Every operation validates all sources first; batch operations report per-item failures instead of claiming blanket success.

`clipboard` owns the in-app structured file clipboard and text path helpers. `explorer`/`platform::shell` open, reveal, show properties, and write the clipboard without constructing command strings from untrusted names; `explorer.exe` receives a single quoted `/select,` argument.

## Duplicate detection

`analysis::duplicates` runs only on request and never modifies files. It groups candidate files by exact byte size (after the configured minimum-size filter), then verifies content in two stages on a scoped worker pool of up to eight threads:

1. **Prefilter.** Files up to 128 KiB are hashed whole; larger files are hashed over their first and last 64 KiB. Each worker reuses one scratch buffer. Files whose prefilter digests differ cannot be duplicates and are dropped without reading their middle.
2. **Full verification.** Only files that collide in the prefilter are read in full and hashed with SHA-256. Hard links or aliases to the same underlying file are recognized by volume serial + file index (`GetFileInformationByHandle`) and do not need another read; they are reported in a group but contribute zero recoverable bytes.

Results are sorted by recoverable bytes. Cancellation is checked between files and chunks and discards the whole result rather than presenting partial duplicate data. Progress reports files and bytes actually read.

## Persistence

`storage::database` wraps SQLite via rusqlite (bundled SQLite, no external service). The database lives in `%APPDATA%\SmartDisk\smartdisk.db` and stores:

- settings (JSON in a key/value table),
- scan history (aggregate stats, not every node),
- recent locations.

WAL mode and a busy timeout are enabled. The database is opened in a background-safe manner; UI reports size and path and can clear scan data. Per-node scan results are kept in memory only and are not persisted.

## UI

The Slint UI (`ui/*.slint`) is a single window with ten pages: Overview, Explorer, Treemap, Largest files, File types, Cleanup, Duplicates, Search, History and Settings, plus an info panel, context menu, cleanup preview, confirmation dialog and toast.

`ui::backend` is the controller: it owns the current `Arc<ScanResult>`, settings, selection, sort state, cleanup plan, duplicate results and pending confirmations. Background work is polled on a timer; expensive analysis (duplicates) runs on its own thread. Keyboard shortcuts live on the root focus scope. Delete maps to Recycle Bin; Shift+Delete maps to permanent deletion with a separate confirmation.

The treemap uses a squarified layout normalized to `[0,1]`; areas are proportional to sizes. Selection is synchronized between treemap, tree and details through retained models.

## Threading summary

| Thread | Work |
|---|---|
| UI thread | Slint event loop and backend callbacks |
| Scan coordinator | `run_scan`; owns workers |
| Scan workers | directory enumeration (bounded pool) |
| Duplicate worker | size grouping, parallel prefilter and SHA-256 verification |
| Shell STA threads | IFileOperation, properties dialog |
| Poll timer | progress and completion updates |

No worker touches UI state directly; results cross thread boundaries through channels and `Arc` snapshots.

## Design decisions

- **Own scanner instead of walking the MFT.** Works on NTFS, FAT/exFAT and network shares without elevation, at the cost of being slower than MFT parsers on very large NTFS volumes.
- **Metadata-only by default.** Predictable I/O and privacy.
- **Independent risk engine.** Classification answers "what is this?"; risk answers "how dangerous is deleting it?" Mixing them made cache classification override protected locations.
- **Flat arena.** Compact memory and no recursive ownership graph.
- **SQLite only for history/settings.** No per-file database writes during scans.
- **Native shell operations.** Conflict/progress/undo behaviour matches Explorer and avoids reimplementing filesystem semantics.
