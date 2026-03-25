#!/usr/bin/env python3
"""
Initialize the unified benchmarks database.

Creates benchmarks.db with:
- Instance metadata (snapshot from meta.db)
- Benchmark results schema (runs + results)
- Competition reference data schema
- Instance features schema

Usage:
    python3 scripts/init_benchmarks_db.py
    python3 scripts/init_benchmarks_db.py --meta-db /path/to/meta.db
    python3 scripts/init_benchmarks_db.py --refresh  # re-snapshot meta.db
"""

import argparse
import shutil
import sqlite3
import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent
DEFAULT_META_DB = Path.home() / "PycharmProjects" / "SpinSAT" / "meta.db"
BENCHMARKS_DB = PROJECT_ROOT / "benchmarks.db"

SCHEMA = """
-- Benchmark runs: one row per official benchmarking session
CREATE TABLE IF NOT EXISTS runs (
    run_id TEXT PRIMARY KEY,
    solver_version TEXT NOT NULL,
    git_commit TEXT NOT NULL,
    git_dirty INTEGER DEFAULT 0,
    integration_method TEXT,
    strategy TEXT,
    timestamp TEXT NOT NULL,
    timeout_s INTEGER NOT NULL,
    hardware TEXT,
    rust_version TEXT,
    tag TEXT,
    notes TEXT,
    restart_strategy TEXT,
    preprocessing TEXT,
    cli_command TEXT,
    tag_purpose TEXT,
    tag_instance_set TEXT,
    tag_config TEXT
);

-- Per-instance results within a run
CREATE TABLE IF NOT EXISTS results (
    run_id TEXT NOT NULL REFERENCES runs(run_id),
    instance_hash TEXT NOT NULL,
    status TEXT NOT NULL,
    time_s REAL,
    steps INTEGER,
    restarts INTEGER,
    verified TEXT,
    seed INTEGER,
    zeta REAL,
    alpha REAL,
    beta REAL,
    gamma REAL,
    delta REAL,
    epsilon REAL,
    dt_min REAL,
    dt_max REAL,
    peak_xl_max REAL,
    final_dt REAL,
    wall_clock_s REAL,
    cpu_time_s REAL,
    num_vars INTEGER,
    num_clauses INTEGER,
    PRIMARY KEY (run_id, instance_hash)
);

-- Competition reference results for comparison
CREATE TABLE IF NOT EXISTS competition_results (
    instance_hash TEXT NOT NULL,
    competition TEXT NOT NULL,
    solver TEXT NOT NULL,
    status TEXT,
    time_s REAL,
    PRIMARY KEY (instance_hash, competition, solver)
);

-- GBD structural features (populated later)
CREATE TABLE IF NOT EXISTS instance_features (
    instance_hash TEXT PRIMARY KEY,
    num_vars INTEGER,
    num_clauses INTEGER,
    ratio REAL,
    family TEXT,
    modularity REAL,
    community_structure REAL
);

-- Materialized year/track lookup from instance_tracks
CREATE TABLE IF NOT EXISTS instance_year_track (
    hash TEXT PRIMARY KEY,
    year INTEGER,
    track_type TEXT
);

-- Best competition solver time per instance (for benchmarked instances)
CREATE TABLE IF NOT EXISTS competition_best (
    instance_hash TEXT PRIMARY KEY,
    best_solver TEXT,
    best_time_s REAL,
    competition TEXT,
    timeout_s INTEGER
);

-- Best solve time per instance across all versions
CREATE VIEW IF NOT EXISTS best_times AS
SELECT
    r.instance_hash,
    MIN(r.time_s) AS best_time,
    ru.solver_version,
    ru.git_commit,
    r.run_id
FROM results r
JOIN runs ru USING(run_id)
WHERE r.status = 'SATISFIABLE'
GROUP BY r.instance_hash;

-- Version comparison: pivot results by solver version
CREATE VIEW IF NOT EXISTS version_comparison AS
SELECT
    r.instance_hash,
    ru.solver_version,
    r.status,
    r.time_s,
    r.steps,
    r.restarts
FROM results r
JOIN runs ru USING(run_id)
ORDER BY r.instance_hash, ru.solver_version;

-- Create indexes for common queries
CREATE INDEX IF NOT EXISTS idx_results_hash ON results(instance_hash);
CREATE INDEX IF NOT EXISTS idx_results_status ON results(status);
CREATE INDEX IF NOT EXISTS idx_runs_version ON runs(solver_version);
CREATE INDEX IF NOT EXISTS idx_runs_timestamp ON runs(timestamp);
CREATE INDEX IF NOT EXISTS idx_competition_hash ON competition_results(instance_hash);
CREATE INDEX IF NOT EXISTS idx_competition_solver ON competition_results(solver);
CREATE INDEX IF NOT EXISTS idx_instance_year_track_year ON instance_year_track(year);
CREATE INDEX IF NOT EXISTS idx_instance_year_track_type ON instance_year_track(track_type);
CREATE INDEX IF NOT EXISTS idx_competition_best_hash ON competition_best(instance_hash);
"""


def snapshot_meta_db(benchmarks_db: Path, meta_db: Path):
    """Copy instance metadata tables from meta.db into benchmarks.db."""
    if not meta_db.exists():
        print(f"Warning: meta.db not found at {meta_db}")
        print("Skipping instance metadata snapshot.")
        print(f"Re-run with: python3 {__file__} --meta-db /path/to/meta.db --refresh")
        return 0

    conn = sqlite3.connect(str(benchmarks_db))
    cursor = conn.cursor()

    # Attach meta.db
    cursor.execute(f"ATTACH DATABASE ? AS meta", (str(meta_db),))

    # Drop existing instance tables if refreshing
    for table in ["instances", "instance_local", "instance_files", "instance_tracks"]:
        cursor.execute(f"DROP TABLE IF EXISTS {table}")

    # Copy features table as instances
    cursor.execute("""
        CREATE TABLE instances AS
        SELECT * FROM meta.features
    """)

    # Copy local table
    cursor.execute("""
        CREATE TABLE instance_local AS
        SELECT * FROM meta.local
    """)

    # Copy filename table
    cursor.execute("""
        CREATE TABLE instance_files AS
        SELECT * FROM meta.filename
    """)

    # Copy track table
    cursor.execute("""
        CREATE TABLE instance_tracks AS
        SELECT * FROM meta.track
    """)

    # Add indexes on the snapshot tables
    cursor.execute("CREATE INDEX IF NOT EXISTS idx_instances_hash ON instances(hash)")
    cursor.execute("CREATE INDEX IF NOT EXISTS idx_instances_family ON instances(family)")
    cursor.execute("CREATE INDEX IF NOT EXISTS idx_instance_tracks_hash ON instance_tracks(hash)")
    cursor.execute("CREATE INDEX IF NOT EXISTS idx_instance_tracks_value ON instance_tracks(value)")
    cursor.execute("CREATE INDEX IF NOT EXISTS idx_instance_local_hash ON instance_local(hash)")
    cursor.execute("CREATE INDEX IF NOT EXISTS idx_instance_files_hash ON instance_files(hash)")

    # Count what we imported
    cursor.execute("SELECT COUNT(*) FROM instances")
    instance_count = cursor.fetchone()[0]

    cursor.execute("SELECT COUNT(DISTINCT value) FROM instance_tracks")
    track_count = cursor.fetchone()[0]

    cursor.execute("SELECT COUNT(DISTINCT family) FROM instances WHERE family != 'empty'")
    family_count = cursor.fetchone()[0]

    cursor.execute("DETACH DATABASE meta")
    conn.commit()
    conn.close()

    print(f"Imported {instance_count} instances, {track_count} tracks, {family_count} families")
    return instance_count


def init_db(benchmarks_db: Path, meta_db: Path, refresh: bool = False):
    """Initialize or refresh the benchmarks database."""
    exists = benchmarks_db.exists()

    if exists and not refresh:
        print(f"Database already exists: {benchmarks_db}")
        print("Use --refresh to re-snapshot meta.db (preserves runs/results)")
        return

    if not exists:
        print(f"Creating {benchmarks_db}")

    conn = sqlite3.connect(str(benchmarks_db))
    conn.executescript("PRAGMA journal_mode=WAL;")
    conn.executescript("PRAGMA foreign_keys=ON;")
    conn.executescript(SCHEMA)
    conn.commit()
    conn.close()

    if not exists:
        print("Schema created: runs, results, competition_results, instance_features")
        print("Views created: best_times, version_comparison")

    count = snapshot_meta_db(benchmarks_db, meta_db)

    if count > 0:
        # Verify
        conn = sqlite3.connect(str(benchmarks_db))
        cursor = conn.cursor()
        cursor.execute("SELECT COUNT(*) FROM instances")
        total = cursor.fetchone()[0]
        cursor.execute("SELECT COUNT(*) FROM runs")
        runs = cursor.fetchone()[0]
        cursor.execute("SELECT COUNT(*) FROM results")
        results = cursor.fetchone()[0]
        conn.close()

        print(f"\nDatabase ready: {benchmarks_db}")
        print(f"  Instances: {total}")
        print(f"  Runs:      {runs}")
        print(f"  Results:   {results}")
    else:
        print(f"\nDatabase ready (no instance data): {benchmarks_db}")


def main():
    parser = argparse.ArgumentParser(description="Initialize SpinSAT benchmarks database")
    parser.add_argument("--meta-db", type=Path, default=DEFAULT_META_DB,
                        help=f"Path to meta.db (default: {DEFAULT_META_DB})")
    parser.add_argument("--refresh", action="store_true",
                        help="Re-snapshot meta.db (preserves existing runs/results)")
    args = parser.parse_args()

    init_db(BENCHMARKS_DB, args.meta_db, args.refresh)


if __name__ == "__main__":
    main()
