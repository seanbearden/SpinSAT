# Benchmarks Database Schema (2026-03-21)

## Location
- Local: `benchmarks.db` (project root, gitignored)
- Distribution: GitHub Releases asset
- Browser access: Datasette Lite link in README

## Setup
```bash
python3 scripts/init_benchmarks_db.py                          # first time
python3 scripts/init_benchmarks_db.py --refresh                # re-snapshot meta.db
python3 scripts/init_benchmarks_db.py --meta-db /path/to/meta.db  # custom path
```

## Tables

### Instance Metadata (snapshot from PycharmProjects/SpinSAT/meta.db)
- `instances` — 31,809 rows (hash, family, author, track, result, etc.)
- `instance_local` — local file paths
- `instance_files` — filenames
- `instance_tracks` — competition track associations (61 tracks, 189 families)

### Benchmark Results
- `runs` — one row per official benchmarking session
  - run_id, solver_version, git_commit, git_dirty, integration_method, strategy
  - timestamp, timeout_s, hardware, rust_version, tag, notes
- `results` — per-instance results within a run
  - run_id, instance_hash, status, time_s, steps, restarts, verified
  - seed, zeta, alpha, beta, gamma, delta, epsilon, dt_min, dt_max

### Competition Reference
- `competition_results` — per-instance solve times from SAT competitions
  - instance_hash, competition, solver, status, time_s
- `instance_features` — GBD structural features (for later)

### Views
- `best_times` — best solve time per instance across all versions
- `version_comparison` — pivot results by solver version

## Key Queries
```sql
-- Best time per instance
SELECT * FROM best_times LIMIT 10;

-- Version-over-version comparison
SELECT instance_hash, solver_version, time_s FROM version_comparison;

-- Instances where v0.4.0 improved over v0.3.0
SELECT ... FROM results JOIN runs USING(run_id) ...
```

## Instance Hash
- SHA-256 of the CNF file contents
- Join key between results and instances tables
- Note: meta.db uses GBD isohash which may differ from SHA-256 file hash
