# Security and safety

SmartDisk is an offline, local-only disk analyzer. This document describes what the application does, what it refuses to do, and the limits of those guarantees.

## Non-negotiable safety rules

- **No automatic destructive cleanup.** SmartDisk never deletes anything without an explicit user action on a previewed plan.
- **No silent deletion.** Every cleanup and file operation reports its result; failures are shown per item.
- **Recycle Bin first.** The normal delete path (Delete key, context menu, cleanup assistant default) moves items to the Recycle Bin.
- **Permanent deletion is separate.** It is only available through Shift+Delete, an explicit context-menu command, or by switching the cleanup preview to permanent mode. It always shows a separate destructive confirmation that names the item count and size.
- **Protected locations are refused.** Drive roots, profile roots, the Windows installation (except narrowly scoped cleanup zones), Program Files, ProgramData\Microsoft, System Volume Information and boot areas cannot be modified through the file-operation layer, even if a UI path tried to request it.
- **No shell command construction from untrusted names.** Copy, move and rename use COM `IFileOperation`. Recycle uses the Windows shell through the `trash` crate. Reveal-in-Explorer passes one quoted `/select,` argument and rejects paths containing a quote. No `cmd /c`, PowerShell or `eval` is built from file names or scanned data.
- **No administrator requirement for ordinary use.** The scanner reads metadata as the current user. The per-user installer requires no elevation.
- **No telemetry and no network access for core functionality.** The application does not phone home, upload file names, or fetch classification data. Classification rules ship in the binary.
- **No destructive testing against real Windows directories.** Automated tests use disposable fixtures under the project directory or the system temp directory.

## Protected-location policy

`PathContext::is_critical_location` matches *path components*, case-insensitively:

- any drive root (`C:\`, `D:\`),
- a user-profile root (`C:\Users\<name>`, not its ordinary subfolders),
- `System Volume Information`,
- top-level boot areas (`\Boot`, `\EFI`, `\Recovery`),
- everything under `\Windows` **except** the narrow cleanup zones: `Windows\Temp`, `Prefetch`, `SoftwareDistribution\Download`, `Logs`, `Minidump`, `LiveKernelReports`,
- everything under `Program Files`, `Program Files (x86)` and `ProgramData\Microsoft`.

The check is applied in three places:

1. The risk engine marks matching items `Critical`.
2. Cleanup recommendations exclude candidates whose containing folder is protected.
3. `file_operations::validate_single` rejects any source path that is protected before any copy, move, rename, recycle or permanent delete is attempted.

Files cannot themselves be a "location", so `is_protected_path` also checks the parent directory for files. This blocks deleting files placed directly in a profile root or drive root (for example `NTUSER.DAT` or `bootmgr`) while leaving ordinary folders such as `Documents` reviewable rather than treatable as system areas.

## Path handling and adversarial input

- Location decisions use component comparison, never substring matching. `C:\WindowsApps` is not `C:\Windows`; `C:\ProgramData\MicrosoftEdge` is not `ProgramData\Microsoft`.
- Lexical `..` components are normalized for absolute paths before protection checks, so `C:\Users\Alice\..\..\Windows\System32` is still recognized as protected.
- Unicode profile names are preserved; matching uses Windows-compatible ASCII case folding for known ASCII components.
- Reparse points (junctions/symlinks) are detected from the reparse attribute. Their targets are not traversed by default, and they are never assigned a lower risk than `High`.
- File names are validated for rename (empty, `.`, `..`, separators, invalid characters, reserved device names, trailing dots/spaces). Rename does not overwrite an existing target.
- Paths containing a double quote are refused by the reveal-in-Explorer path rather than passed to a command line.

These checks reduce risk but are not a security boundary against a malicious *local user* who can replace files between validation and operation (a time-of-check/time-of-use race). SmartDisk is a user-facing tool, not an EDR product.

## File operation semantics

| Operation | Mechanism | Notes |
|---|---|---|
| Copy / move | `IFileOperation` on an STA thread | Native conflict/progress UI; per-item failures reported |
| Rename | `IFileOperation` | Name validation, no overwrite |
| Recycle | `trash` crate / native shell | Reversible through the Recycle Bin |
| Permanent delete | `remove_file` / `remove_dir_all`, reparse-safe | Separate confirmation; removes read-only attribute first |

All sources are validated as a batch before an operation starts. If the shell reports a batch failure, queued items are moved to the failed list rather than being reported as successes. Reparse points are removed as links, never as their target trees.

## Data at rest

SmartDisk stores settings, scan history (aggregate statistics only) and recent locations in `%APPDATA%\SmartDisk\smartdisk.db` using bundled SQLite. Per-file scan results stay in memory and are discarded when the application exits. The database contains paths you scanned and aggregate counts/sizes. It is not encrypted; use the Settings page to clear scan data or delete the folder manually.

Duplicate detection reads file *contents* locally to compute SHA-256. Contents are never uploaded or written anywhere. Hashing is only started when the user presses the button.

## Network

The core application makes no network calls. The UI footer states "Offline · no telemetry". Building the application and downloading Rust crates require network access, but the installed application does not.

## Installer and binaries

- The installer is per-user and installs under `%LOCALAPPDATA%\Programs\SmartDisk` by default, so it does not need administrator rights.
- The installer and executable are **not code-signed**. Windows SmartScreen may warn on first run. Verify `checksums.txt` and build from source if that matters to you.
- Uninstalling removes application files; `%APPDATA%\SmartDisk` user data is intentionally left behind.

## Error handling

- A missing root, a file passed as a root, or an unreadable root returns an actionable error and no bogus empty result.
- Unreadable subdirectories are recorded as `ScanIssue`s; the scan continues and ends with `Partial` status. The UI displays the error count.
- Cancelled scans are labelled `Cancelled` and their partial totals are never presented as complete.
- File-operation errors are shown to the user with the affected path; they are not swallowed.

## What SmartDisk does not protect against

- A user with administrator rights can delete anything outside the application. SmartDisk's policy applies to operations it performs, not to the operating system.
- Files can change while a scan runs; results are a snapshot. A file shown as small may have grown before deletion, and vice versa.
- The Recycle Bin can be disabled or too small on some volumes; Windows may then permanently delete on a normal "recycle" request. SmartDisk does not control that behaviour.
- Reparse points can point to protected or unexpected locations. SmartDisk does not follow them by default; if you enable following, treat totals and cleanup candidates with extra care.
- The application cannot verify that a third-party file is safe to delete; classification is evidence-based and can be wrong.

## Reporting

This is a local project without a hosted security contact. If you find a safety defect, reproduce it against disposable test data, capture the exact path and operation, and report it to the project maintainer. Do not test destructive paths against real Windows or user data.
