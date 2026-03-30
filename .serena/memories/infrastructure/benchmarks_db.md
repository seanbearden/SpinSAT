# Benchmarks Database (Updated 2026-03-29)

## IMPORTANT: Cloud SQL is the single source of truth
See `infrastructure/cloud_sql` memory. Local SQLite is only for GitHub Pages export.

## Local SQLite (benchmarks.db)
- Location: project root (gitignored, distributed via GitHub Releases)
- Used by: GitHub Pages dashboard (sql.js), offline analysis
- Export from Cloud SQL: `python3 scripts/migrate_to_cloud_sql.py` (reverse not yet built)

## Schema (same in both SQLite and PostgreSQL)
- `runs` — benchmark sessions (run_id, solver_version, git_commit, tag, timeout, etc.)
- `results` — per-instance results (status, time_s, parameters, peak_xl_max, etc.)
- `competition_results` — 153K rows (SAT 2017/2018 random track + anni2022 Anniversary)
- `instances` — 31K rows GBD metadata
- `instance_files` — filename-to-hash mappings
- `family_params` — per-family best Optuna parameters
- `best_times` — view of best solve time per instance

## Instance Hash
GBD hash extracted from filename prefix (`<32-hex>-<name>.cnf`), NOT SHA-256 of content.
`compute_instance_hash()` in benchmark_suite.py handles this.
This is critical for head-to-head queries against competition_results.

## Competition Reference Data
- anni2022 Anniversary Track: 149,940 rows (28 solvers x 5,355 structured instances)
- SAT 2017 Random Track: 901 rows (3 solvers x 300 instances — barthel, komb, qhid, uniform)
- SAT 2018 Random Track: 2,552 rows (10 solvers x 255 instances — barthel, komb, qhid, uniform)
- Downloaded from satcompetition.github.io/2017/results/random.csv and /2018/results/random.csv

## Key: No overlap between random and structured instances
Our benchmark results are on random instances (barthel, komb, qhid).
The anni2022 competition data covers structured/crafted instances.
Different families, zero hash overlap. Head-to-head only works within same family.
