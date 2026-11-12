# SmartDisk duplicate detection benchmark

- Environment: Microsoft Windows 11 Pro for Workstations build 22631; Intel(R) Xeon(R) CPU E3-1545M v5 @ 2.90GHz; 15.9 GB RAM; logical processors: 8; Intel64 Family 6 Model 94 Stepping 3, GenuineIntel
- Build profile: release
- Fixture: 160 files of 1 MiB forming 80 duplicate pairs; 100 files of 512 KiB that share a size but differ in the first byte; 20 files of 256 KiB each with a hard link (40 paths).
- Baseline: the 1.0.1 algorithm - sequential full-file SHA-256 for every same-size candidate, 1 MiB buffer allocated per file.
- Optimized: parallel two-stage hashing (head/tail prefilter, full hash only for prefilter collisions) with hard-link identity short-circuit. Each worker reuses one buffer.
- Method: one in-memory scan result; median of 3 runs for each implementation on the same machine.

| files | logical bytes | content groups | sequential full hash ms | optimized ms | speedup | bytes read (optimized) |
|---:|---:|---:|---:|---:|---:|---:|
| 300 | 230686720 | 100 | 1303 | 577 | 2.3x | 207093760 (89.8% of logical) |

- Files hashed (prefilter pass): 300; errors: 0.
