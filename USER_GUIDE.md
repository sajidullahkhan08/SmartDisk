# SmartDisk user guide

SmartDisk answers three questions: **where did my disk space go**, **what are these items**, and **what is safe to remove**. It scans metadata only and works entirely offline.

## Installing and starting

Run `SmartDisk-Setup.exe` from the release folder. The installer is per-user and does not require administrator rights. It creates a Start menu entry, an optional desktop shortcut, and an uninstaller. You can also run `SmartDisk.exe` directly without installing.

On first start, SmartDisk opens the Overview page with no scan. Choose a drive from the header list, type a folder path, or press **Browse…**.

## Scanning

1. Enter a drive or folder path, for example `D:\Projects` or `C:\Users\You\Downloads`.
2. Press **Scan** (or Enter in the path field). F5 rescans the current location.
3. While scanning you can **Pause**, **Resume** or **Cancel**. Cancelling keeps partial results but labels them as incomplete.
4. Unreadable subfolders do not abort the scan. The result shows an error count and a `Partial` status; those folders' contents are not included in totals.

SmartDisk does not require elevation. Without administrator rights some system folders will be unreadable; that is reported, not hidden.

## The header

| Control | Purpose |
|---|---|
| Drive list | Quick selection of available local volumes |
| Path field | Drive or folder to scan; Enter starts the scan |
| Browse… | Folder picker |
| Scan / Rescan | Start a scan (F5) |
| Pause / Resume, Cancel | Control a running scan |
| Rescan button | Rescan the current location (F5) |
| Search field | Shows the global search query when a scan exists |
| Info toggle | Show or hide the details panel |

## Pages

### Overview

Total logical size, allocated size (cluster-rounded), file/folder counts, error count, elapsed time, average file size and volume usage. The largest file and folder are shown with shortcuts to reveal them. **Space by category** lists categories sorted by size; clicking one opens Search filtered to that category. **Top-level folders** shows direct children of the scanned root with their share.

### Explorer

The folder tree on the left and the item table on the right. Columns: name, size, allocated, modified, category, risk, and files/folders. Click a header to sort; click a folder to browse into it.

Actions:

- **Double-click** a folder to open it; double-click a file to open it with its default program.
- **Right-click** for the context menu: Open, Open with, Reveal in Explorer, Properties, Copy, Cut, Rename, Recycle, Delete permanently.
- **Ctrl+C / Ctrl+X / Ctrl+V** copy, cut and paste into the current folder.
- **F2** renames; invalid names and existing targets are rejected.
- **Delete** moves the selection to the Recycle Bin. **Shift+Delete** permanently deletes after a separate confirmation.
- **Go up** and breadcrumbs navigate the hierarchy.

Selecting an item fills the info panel (see below). File operations are performed by Windows' own shell, so conflict prompts look like Explorer's.

### Treemap

A squarified treemap of the current folder's children. Rectangle area is proportional to size; color follows the classification category. Use the breadcrumb to go back, or double-click a folder rectangle to zoom into it. Selection is synchronized with the tree and details. Empty folders and zero-byte items do not receive area.

Treemap totals are logical sizes, not allocated sizes, and exclude reparse-point targets that were not followed.

### Largest files

The largest files in the scan, sortable by size, allocated size, name, location and category. Use the **Show** action to reveal a file in Explorer. Selecting a row opens its details.

### File types

Space by category and the most common extensions, with counts and totals. Categories and extensions can be clicked to search for matching items.

### Cleanup

Scan first. SmartDisk groups recognized **temporary files**, **application and browser caches**, **logs**, **Windows Update downloads** and **Recycle Bin contents**. Each group shows its estimated size and item count.

1. Select one or more groups. **Select all** and **Clear** are available.
2. Press **Preview**.
3. Review the plan: every item path, size and risk level, plus the recovery estimate and any warnings.
4. Keep **Move to Recycle Bin** selected (default), or deliberately switch to permanent deletion. The button label changes accordingly.
5. Confirm. Permanent mode always shows an additional destructive confirmation.
6. SmartDisk reports how many items were processed and rescans the location.

Notes:

- Cleanup never runs automatically. Nothing is selected by default.
- Folders are not cleanup candidates; only their files are, so contents are not counted twice.
- Protected locations are excluded. High-risk user data is never offered.
- "Reclaimable" is an estimate. Cluster rounding, hard links, files in use and regenerated caches can change the real result.
- Files in use may fail individually; the result lists failures.

### Duplicates

Duplicate detection is optional and runs only when you press **Find duplicates**. It groups files by exact size, then verifies contents on a small worker pool: a fast head/tail check rules out files that differ near the start or end, and only files that still match are read in full and hashed with SHA-256. Hard links to the same file are recognized and not counted as separate copies. The minimum size (configured in Settings, default 1 MB) limits the work. Progress shows files and bytes actually read.

Groups show the file count, size and recoverable bytes. Hard links to the same underlying file are recognized and not counted multiple times. You can reveal a copy or move selected copies to the Recycle Bin. Detection can be cancelled; a cancelled run never presents partial groups as complete.

Hashing reads file contents locally. Large scans and slow disks can make this take a while; use the minimum-size setting to keep it practical.

### Search

Searches the in-memory scan. The query matches names, or full paths when it contains `\` or `/`. Filters:

- category,
- risk level,
- kind (files/folders),
- minimum size,
- extension,
- result limit (the UI uses a bounded number).

Results are sortable and support the same context actions as Explorer. Ctrl+F jumps here and focuses the query.

### History

Recent scans with path, time, status, total size, item counts and elapsed time. **Rescan** repeats a location; **Clear history** removes stored entries. History stores aggregate statistics, not your file list. SmartDisk also remembers recent locations for the header path suggestions.

### Settings

- **Theme** — dark or light.
- **Low-resource mode** — limits scan workers to at most two.
- **Scan workers** — `0` selects automatically (CPUs − 1, clamped to 1–8); a number forces that many workers (1–32).
- **Follow reparse points** — off by default. Enabling it follows junctions/symlinks and tracks visited targets to avoid cycles; totals can then include other volumes.
- **Exclusions** — one absolute path per line. Matching is component-based; excluded subtrees are skipped without an error.
- **Minimum duplicate size** — in MB.
- **Use Recycle Bin by default** — should stay on unless you have a specific reason.
- **Confirm destructive actions** — ask before cleanup and destructive file operations.
- **Remember last location** — restores the last scanned path.
- **Open data folder** — opens `%APPDATA%\SmartDisk`.
- **Clear scan data** — removes history and recent locations from the database.

## The info panel

Select any item to see:

- name, full path, kind, logical and allocated size, modification time,
- classification category and confidence,
- **deletion risk** (`Critical`, `High`, `Review`, `Lower`, `Unknown`),
- a plain-language summary, possible consequences and a recommendation,
- the evidence used (path components, extension, attributes, age, matched rule).

Risk is about **deleting the item**, not about whether SmartDisk likes it:

| Level | Meaning |
|---|---|
| Critical | Protected system/profile location. Deletion is refused. |
| High | Important user, application or project data. No routine cleanup recommendation. |
| Review | Recognized temporary/cache/log data with caveats. Inspect first. |
| Lower | Strong evidence of disposable temp/cache data (files only, age-qualified). |
| Unknown | Not enough evidence. No recommendation. |

## File operations and undo

Normal deletion goes to the Recycle Bin and can be restored there. Permanent deletion cannot be undone. Copy, move and rename use Windows' native shell dialogs for conflicts and progress. If an operation partially fails, SmartDisk lists the failed items and the reason.

## Troubleshooting

**A folder shows errors.** It is unreadable with your current permissions. Contents are excluded and marked in the result. Do not elevate just to analyze; check whether the folder matters first.

**The scan is slow.** Large folders with many small files are metadata-heavy. Lower the worker count if the machine becomes unresponsive, or use exclusions and scan a narrower folder.

**Cleanup shows nothing.** Recommendations only cover recognized temporary/cache/log/update/Recycle Bin files that are eligible by risk. A clean system or a folder without recognized locations will have no suggestions.

**Duplicate detection is slow.** It reads full file contents for same-size candidates. Raise the minimum size, narrow the scan, or skip it.

**A file cannot be renamed or moved.** It may be protected, in use, read-only, or the name may be reserved/invalid. The error message identifies the path.

**The UI warns about SmartDisk at launch.** The binaries are unsigned; SmartScreen may warn. Build from source or verify checksums if that is a concern.

**Where is my data?** Settings and history are in `%APPDATA%\SmartDisk\smartdisk.db`. Scan results are memory-only and disappear on exit.

## Keyboard summary

F5 rescan · Ctrl+F search · Ctrl+C/X/V copy/cut/paste · F2 rename · Delete recycle · Shift+Delete permanent · Escape close dialog/menu.
