# The Story Behind SmartDisk

*Why I built a disk analyzer that tells you not just where your space went — but what is actually safe to delete.*

---

## The problem I kept running into

Every few months, the same thing happened: my disk filled up.

I would fire up a tool like **WizTree**. It is genuinely impressive — it reads the NTFS master file table and in seconds shows a beautiful treemap of what is eating the disk. Big rectangles, small rectangles, colors, zoom. "There is your answer," it seems to say.

Except it wasn't the answer I needed.

WizTree told me **where** the space went. It did not tell me **what any of it was**, **whether deleting it would break something**, or **what the consequences would be**. A giant folder under `ProgramData` — is that a cache I can clear, or a database my applications will miss? Twenty gigabytes in `Windows\Installer` — safe, or the reason my next update fails? A folder with a vague name deep in `AppData` — temporary junk, or my saved game?

So I did what most people do: I hovered over the rectangles, Googled folder names for an hour, got three contradictory answers, and closed the tool with the disk still full. The fear of deleting the wrong thing was worse than the missing space. The tool made the disk *visible*, but not *understandable*.

That was the realization:

> **Knowing where space went is not the same as knowing what to do about it.**
> The missing piece is an explanation — and a safety model strong enough to act on it.

## What I decided to build

I wrote down the requirements before writing a single line of code. Whatever I built had to:

1. **Explain itself.** Every classification — cache, log, installer, document, system file — must come with *evidence*: which path components, name patterns or extensions led to that conclusion. No black-box "AI says junk."
2. **Separate guessing from safety.** Classification ("this looks like a cache") and deletion risk ("deleting this is dangerous") are different questions. They must be answered by two independent engines, so a wrong guess about a file's purpose can never put a system path at risk.
3. **Never be reckless.** No automatic cleanup, no silent deletes, Recycle Bin by default, permanent deletion behind a separate opt-in and a second confirmation, and a hard-coded list of protected locations (drive roots, profile roots, Windows, Program Files, boot areas) that no recommendation may ever touch.
4. **Work completely offline.** My disk contents are my business. No telemetry, no upload, no account, no "cloud features."
5. **Feel native and fast.** A real Windows application — small, snappy, dark/light themes — not a wrapped web page. Scanning a drive should take seconds, not minutes.
6. **Clean, not just analyze.** An assistant that groups genuinely safe candidates (temporary files, caches, logs, update leftovers, Recycle Bin) into a previewed plan with a recovery estimate — so cleanup becomes a reviewed decision, not a leap of faith.

That became **SmartDisk**.

## How it works

SmartDisk is built around three ideas that most disk tools skip.

### 1. Offline, explainable classification

SmartDisk ships with a hand-written taxonomy of file and folder categories — system files, installers, caches, logs, databases, archives, documents, media, development dependencies, virtual-machine disks, cloud-sync data, and more. The classifier looks at **path components** (`C:\Users\me\AppData\Local\Temp`), **filename patterns**, and **extensions**, and produces a category plus a confidence score.

Crucially, every result carries its evidence: *"Location matches a per-user temporary directory. Path components: Users > me > AppData > Local > Temp."* If the evidence is weak, the answer is *Unknown / Needs Review* — SmartDisk prefers to admit uncertainty over pretending certainty.

Everything runs locally. Nothing is sent anywhere.

### 2. An independent, conservative risk engine

Deletion risk is computed by a **separate** engine. A file being classified as "cache" does not make it safe — a cache inside the Windows directory is not yours to clean. The risk engine evaluates the location itself: drive roots, user-profile roots, the Windows installation, Program Files, ProgramData\Microsoft, System Volume Information and boot areas are protected, full stop. Adversarial paths (`..` tricks, look-alike names like `Windows.old2`) cannot disguise a protected location.

Only when *both* engines agree — recognized temporary content, in a recognized location, with appropriate age — does an item become cleanup-eligible, and even then it is labeled `Lower` or `Review`, never presented as certainly safe.

### 3. A cleanup assistant that asks before it acts

The Cleanup page groups eligible items (temporary files, caches, logs, Windows Update leftovers, Recycle Bin contents) and shows a plan: what will be moved, how much space is expected back. The default action is the **Recycle Bin**. Permanent deletion requires switching modes explicitly and confirming again. After execution, SmartDisk rescans so you see the actual result.

## The engineering journey

The first version (1.0.0) proved the concept end to end: a bounded-parallel metadata scanner, the ten-page UI (Overview, Explorer, treemap, Largest files, File types, Cleanup, Duplicates, Search, History, Settings), SQLite history, and the full test suite.

Then real use exposed what to fix:

- **1.0.1 — UI polish.** The maximize button didn't work, header controls were cramped, and some fonts were too small. Fixed with layout rework and verified with screenshots in windowed and maximized states.
- **1.0.2 — Performance.** Scanning felt slow next to WizTree's MFT trick, and duplicate detection crawled on large sets. I replaced directory enumeration with `FindFirstFileExW` (large-fetch, no 8.3-name lookups), removed per-entry allocations from the scan hot path, and rewrote duplicate hashing as a parallel, two-stage process (head/tail prefilter, full SHA-256 only on collisions, hard links recognized by file identity). Same results, much faster: 100,000 files in 71 ms, and a 2.3× faster duplicate pass on a 230 MB fixture.
- **1.0.3 — Production release.** A final UI pass (unified location bar, readability), the full project published here on GitHub, and a proper public download.

Throughout, I refused to let unverifiable claims ship: every performance number in this repository is measured and reproducible (`PERFORMANCE.md`), every release was installed, launched and uninstalled for real (`TESTING.md`), and features that couldn't be honestly verified are labeled **Not Verified** rather than implied done.

## What I'd tell someone in my original position

WizTree is excellent at what it does. If you only want to *see* your disk, use it — it will show you the answer in seconds.

But if you want to *understand* your disk — to know what that 20 GB folder actually is, why it was classified that way, whether deleting it is safe, what it would affect, and to clean up with a reviewed plan instead of a nervous guess — that's the tool I wanted, so that's the tool I built.

SmartDisk is the disk analyzer that explains itself.

---

*See [README.md](README.md) for what SmartDisk does, [ARCHITECTURE.md](ARCHITECTURE.md) for how it is built, [SECURITY.md](SECURITY.md) for the safety policy, and [PERFORMANCE.md](PERFORMANCE.md) for measured numbers.*
