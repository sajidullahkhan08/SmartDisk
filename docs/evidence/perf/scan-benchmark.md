# SmartDisk scan benchmark

- Environment: Microsoft Windows 11 Pro for Workstations build 22631; Intel(R) Xeon(R) CPU E3-1545M v5 @ 2.90GHz; 15.9 GB RAM; logical processors: 8; Intel64 Family 6 Model 94 Stepping 3, GenuineIntel
- Build profile: release
- Method: 1 warm-up scan(s) plus 3 measured scans per scope; median reported. Peak working set is process-wide and cumulative.
- database after recording a 100000-file scan: 136.6 KiB 

| scope | dirs | files | logical bytes | scan ms (median) | files/s | CPU ms | WS before / after (MB) | peak WS (MB) | treemap ms | search ms | classify 10k ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| small | 10 | 200 | 102400 | 2 | 88390 | 0 | 4.7 / 4.8 | 5.3 | 0.002 | 0.126 | 0.4 |
| medium | 100 | 10000 | 2560000 | 8 | 1204413 | 0 | 8.3 / 9.8 | 11.1 | 0.010 | 1.152 | 21.1 |
| deep | 250 | 1000 | 64000 | 56 | 17700 | 16 | 8.4 / 8.6 | 11.1 | 0.000 | 0.056 | 44.3 |
| many-small | 40 | 30000 | 0 | 12 | 2467369 | 16 | 12.8 / 16.7 | 18.9 | 0.000 | 5.942 | 20.4 |
| large | 500 | 50000 | 6400000 | 34 | 1457743 | 31 | 15.8 / 21.9 | 24.4 | 0.045 | 5.637 | 21.1 |
| very-large | 1000 | 100000 | 6400000 | 71 | 1408693 | 47 | 23.3 / 35.6 | 39.9 | 0.093 | 12.095 | 21.2 |
