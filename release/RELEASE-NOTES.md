# SmartDisk 1.0.3 — release notes

SmartDisk is a lightweight, offline disk-space analyzer for Windows. This is a UI polish and documentation release: the header location bar was redesigned, readability was improved across the interface, and the documentation was rewritten for the public release. No functional, scanner or risk-engine changes.

## What's new in 1.0.3

- Redesigned location bar. The drive picker and the path field now share a single bordered bar, so the address is no longer shown twice. The drive dropdown shows a compact label (`C:`) and the full path lives in one place; the bar highlights when the path field has keyboard focus.
- Readability pass: larger stat values on the Overview cards, clearer section titles, slightly taller buttons and navigation items, and more generous empty states.
- The empty-state action button ("Scan selected location") is wide enough to show its full label.
- New README with screenshots and a download section, and STORY.md — the motivation and methodology behind SmartDisk: what tools like WizTree do not tell you (what a file is, whether deleting it is safe, and what it affects) and how SmartDisk answers those questions.

See `CHANGELOG.md` for the full history.

## Highlights

- Scan a drive or folder without administrator rights; pause, resume or cancel at any time.
- See where space went with an Overview dashboard, Explorer-style tree, interactive treemap, largest-files view and file-type breakdown.
- Understand each item before deleting it: category, confidence, evidence, deletion risk, consequences and recommendation.
- Clean temporary/cache/log data through a previewed plan. The Recycle Bin is the default; permanent deletion requires switching mode and a second confirmation.
- Search the whole scan and optionally find duplicates by SHA-256 with hard-link awareness.
- Everything runs locally. No telemetry, no network access, no Python/Node/browser/runtime database.

## Install

1. Run `SmartDisk-Setup.exe`.
2. Optionally enable the desktop shortcut.
3. Launch from the Start menu.

The installer installs per user under `%LOCALAPPDATA%\Programs\SmartDisk`, so it does not need elevation. A portable copy of `SmartDisk.exe` also works. Uninstalling leaves settings/history in `%APPDATA%\SmartDisk`; remove that folder manually if desired.

`checksums.txt` lists the SHA-256 of `SmartDisk-Setup.exe` and `SmartDisk.exe`. Verify with `certutil -hashfile <file> SHA256`.

## Requirements

- Windows 10 or later, x64.
- No other runtime is required for core functionality. The C runtime is linked statically; no Visual C++ Redistributable is needed.

## Known issues

- The executable and installer are unsigned. Windows SmartScreen may warn.
- Allocated sizes use an estimated 4096-byte cluster and are not adjusted for sparse/compressed files or hard links.
- Junctions/symlinks are not followed by default.
- Native shell conflict/progress dialogs are not covered by automated UI tests.
- The UI is English-only; accessibility and high-DPI behaviour were not exhaustively tested.
- The installer wizard was verified only through silent installation; standard-user and Windows 10 environments were not tested.
- The scanner reads directory metadata rather than the NTFS MFT. More speed on very large NTFS volumes would require a privileged MFT reader, which is outside the no-admin design.

See `KNOWN_LIMITATIONS.md` for the complete list and `TESTING.md` for verification status.

## Verification

- Debug test suite on the 1.0.3 build: 128 tests passed, 0 failed (scanner, classification, risk, aggregation, file operations, duplicate detection, end-to-end workflow). The opt-in performance benchmarks are excluded from that count.
- Release build produced `SmartDisk.exe` (22,523,392 bytes) with version resource 1.0.3.0 and static C runtime; imports are Windows system DLLs only.
- The redesigned location bar and the readability changes were verified visually with screenshots (`docs/screenshots/`) on the release build.
- Installer compiled with Inno Setup 6.7.3 from `installer\smartdisk.iss` for version 1.0.3 and verified by silent per-user install, launch and uninstall on Windows 11 build 22631.
- Not verified: true access-denied scanning under a non-privileged account, installer wizard UI, code signing/SmartScreen, Windows 10, and real full-volume scans. These are listed as Not Verified in `TESTING.md`.

## License

MIT. See `LICENSE`.
