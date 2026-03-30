# Cloud SQL — Single Source of Truth (2026-03-29)

## Instance
- Name: `spinsat-optuna` on GCP project `spinsat`
- IP: `34.57.20.164`
- Tier: `db-g1-small` (0.6GB RAM, max_connections=100)
- Region: `us-central1`
- Backups: enabled (7-day retention, 4AM UTC)
- Deletion protection: enabled

## Databases
1. `optuna` — Optuna study storage (existing)
   - User: `optuna`
2. `spinsat_benchmarks` — benchmark results, competition data, instance metadata
   - User: `benchmarks`
   - Same password as optuna (stored in `optuna_studies/.db-password-spinsat-optuna`)

## spinsat_benchmarks Schema
- `runs` — benchmark sessions (run_id, solver_version, git_commit, tag, timeout, etc.)
- `results` — per-instance results (status, time_s, parameters, peak_xl_max, etc.)
- `competition_results` — 153K rows from SAT 2017/2018 random + anni2022 Anniversary Track
- `instances` — 31K rows GBD metadata (hash, family, track)
- `instance_files` — 36K filename-to-hash mappings
- `family_params` — per-family best parameters from Optuna tuning
- `best_times` — view: best solve time per instance

## Connection
```python
import psycopg2
pw = open('optuna_studies/.db-password-spinsat-optuna').read().strip()
conn = psycopg2.connect(host='34.57.20.164', dbname='spinsat_benchmarks', user='benchmarks', password=pw)
```
Or set `SPINSAT_DB_URL` env var.

## Key Rule
Cloud SQL is the SINGLE SOURCE OF TRUTH. Local SQLite (benchmarks.db) is only for GitHub Pages dashboard export. All new runs must record to Cloud SQL first.

## Instance Hash Scheme
- GBD hash extracted from filename prefix: `<32-char-md5>-<name>.cnf`
- `compute_instance_hash()` in benchmark_suite.py handles this automatically
- Competition data and our results use the same GBD hashes — head-to-head queries work

## Connection Budget (100 max)
- Optuna workers: pool_size=2 per worker
- Benchmark workers: pool_size=2 per worker  
- Local dev: 2-4 connections
- Peak ~30 connections with 4+4 concurrent workers
