# SmartDisk scan benchmark

- Environment: Microsoft Windows 11 Pro for Workstations build 22631; Intel(R) Xeon(R) CPU E3-1545M v5 @ 2.90GHz; 15.9 GB RAM; logical processors: 8; Intel64 Family 6 Model 94 Stepping 3, GenuineIntel
- Build profile: release
- Method: 1 warm-up scan(s) plus 3 measured scans per scope; median reported. Peak working set is process-wide and cumulative.
- database after recording a 100000-file scan: 136.6 KiB 

| scope | dirs | files | logical bytes | scan ms (median) | files/s | CPU ms | WS before / after (MB) | peak WS (MB) | treemap ms | search ms | classify 10k ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| small | 10 | 200 | 102400 | 2 | 97714 | 0 | 5.0 / 5.0 | 5.5 | 0.002 | 0.126 | 0.4 |
| medium | 100 | 10000 | 2560000 | 9 | 1073203 | 0 | 8.7 / 10.0 | 11.3 | 0.015 | 2.115 | 21.1 |
| deep | 250 | 1000 | 64000 | 61 | 16467 | 16 | 8.5 / 8.7 | 11.3 | 0.001 | 0.060 | 55.1 |
| many-small | 40 | 30000 | 0 | 12 | 2539683 | 0 | 12.8 / 16.8 | 19.4 | 0.000 | 7.404 | 22.2 |
| large | 500 | 50000 | 6400000 | 52 | 964439 | 78 | 16.2 / 22.5 | 26.7 | 0.066 | 10.111 | 23.8 |
| very-large | 1000 | 100000 | 6400000 | 69 | 1447568 | 62 | 23.3 / 35.5 | 40.2 | 0.093 | 12.189 | 19.4 |
