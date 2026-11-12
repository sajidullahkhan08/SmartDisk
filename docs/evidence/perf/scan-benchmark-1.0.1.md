# SmartDisk scan benchmark

- Environment: Microsoft Windows 11 Pro for Workstations build 22631; Intel(R) Xeon(R) CPU E3-1545M v5 @ 2.90GHz; 15.9 GB RAM; logical processors: 8; Intel64 Family 6 Model 94 Stepping 3, GenuineIntel
- Build profile: release
- Method: 1 warm-up scan(s) plus 3 measured scans per scope; median reported. Peak working set is process-wide and cumulative.
- database after recording a 100000-file scan: 136.6 KiB 

| scope | dirs | files | logical bytes | scan ms (median) | files/s | CPU ms | WS before / after (MB) | peak WS (MB) | treemap ms | search ms | classify 10k ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| small | 10 | 200 | 102400 | 3 | 70368 | 0 | 5.0 / 5.0 | 5.3 | 0.002 | 0.117 | 0.5 |
| medium | 100 | 10000 | 2560000 | 11 | 870390 | 16 | 8.6 / 9.7 | 11.4 | 0.009 | 1.115 | 22.0 |
| deep | 250 | 1000 | 64000 | 80 | 12429 | 16 | 8.6 / 8.8 | 11.4 | 0.000 | 0.096 | 45.4 |
| many-small | 40 | 30000 | 0 | 19 | 1594447 | 62 | 13.2 / 17.0 | 19.4 | 0.000 | 7.489 | 25.0 |
| large | 500 | 50000 | 6400000 | 42 | 1199400 | 31 | 16.9 / 22.8 | 25.1 | 0.048 | 6.008 | 25.7 |
| very-large | 1000 | 100000 | 6400000 | 84 | 1194221 | 47 | 23.5 / 35.9 | 42.1 | 0.099 | 12.772 | 23.6 |
