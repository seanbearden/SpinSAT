#!/usr/bin/env python3
"""
Initialize the spinsat_benchmarks PostgreSQL schema on Cloud SQL.

Creates all tables for benchmark results, competition reference data,
work queue, and family parameters.

Usage:
    python3 scripts/init_benchmarks_pg.py
    python3 scripts/init_benchmarks_pg.py --db-url postgresql://user:pass@host/db
"""

import argparse
import os
import sys

try:
    import psycopg2
except ImportError:
    print("Error: psycopg2 not installed. Run: pip install psycopg2-binary")
    sys.exit(1)

PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def get_db_url():
    """Get database URL from env or password file."""
    url = os.environ.get("SPINSAT_DB_URL")
    if url:
        return url
    pw_file = os.path.join(PROJECT_ROOT, "optuna_studies", ".db-password-spinsat-optuna")
    if os.path.exists(pw_file):
        password = open(pw_file).read().strip()
        return f"postgresql://benchmarks:{password}@34.57.20.164:5432/spinsat_benchmarks"
    print("Error: Set SPINSAT_DB_URL or ensure optuna_studies/.db-password-spinsat-optuna exists")
    sys.exit(1)


SCHEMA_SQL = """
-- Schema version tracking
CREATE TABLE IF NOT EXISTS schema_version (
    version     INTEGER PRIMARY KEY,
    applied_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    description TEXT
);

-- Benchmark runs
CREATE TABLE IF NOT EXISTS runs (
    run_id              TEXT PRIMARY KEY,
    solver_version      TEXT NOT NULL,
    git_commit          TEXT,
    git_dirty           BOOLEAN DEFAULT FALSE,
    integration_method  TEXT,
    strategy            TEXT,
    timestamp           TIMESTAMPTZ NOT NULL,
    timeout_s           INTEGER NOT NULL,
    hardware            TEXT,
    rust_version        TEXT,
    tag                 TEXT,
    notes               TEXT,
    restart_strategy    TEXT,
    preprocessing       TEXT,
    cli_command         TEXT
);

CREATE INDEX IF NOT EXISTS idx_runs_version ON runs(solver_version);
CREATE INDEX IF NOT EXISTS idx_runs_timestamp ON runs(timestamp);

-- Per-instance results
CREATE TABLE IF NOT EXISTS results (
    run_id          TEXT NOT NULL REFERENCES runs(run_id),
    instance_hash   TEXT NOT NULL,
    status          TEXT NOT NULL,
    time_s          DOUBLE PRECISION,
    steps           INTEGER,
    restarts        INTEGER,
    verified        TEXT,
    seed            INTEGER,
    zeta            DOUBLE PRECISION,
    alpha           DOUBLE PRECISION,
    beta            DOUBLE PRECISION,
    gamma           DOUBLE PRECISION,
    delta           DOUBLE PRECISION,
    epsilon         DOUBLE PRECISION,
    dt_min          DOUBLE PRECISION,
    dt_max          DOUBLE PRECISION,
    peak_xl_max     DOUBLE PRECISION,
    final_dt        DOUBLE PRECISION,
    wall_clock_s    DOUBLE PRECISION,
    cpu_time_s      DOUBLE PRECISION,
    num_vars        INTEGER,
    num_clauses     INTEGER,
    cdcl_handoffs   INTEGER,
    solved_by       TEXT,
    xl_decay        DOUBLE PRECISION,
    restart_noise   DOUBLE PRECISION,
    alpha_initial   DOUBLE PRECISION,
    alpha_up_mult   DOUBLE PRECISION,
    alpha_down_mult DOUBLE PRECISION,
    alpha_interval  DOUBLE PRECISION,
    restart_mode    TEXT,
    strategy_used   TEXT,
    preprocess_enabled BOOLEAN,
    PRIMARY KEY (run_id, instance_hash)
);

CREATE INDEX IF NOT EXISTS idx_results_hash ON results(instance_hash);
CREATE INDEX IF NOT EXISTS idx_results_status ON results(status);

-- Competition reference results
CREATE TABLE IF NOT EXISTS competition_results (
    instance_hash   TEXT NOT NULL,
    competition     TEXT NOT NULL,
    solver          TEXT NOT NULL,
    status          TEXT,
    time_s          DOUBLE PRECISION,
    PRIMARY KEY (instance_hash, competition, solver)
);

CREATE INDEX IF NOT EXISTS idx_comp_hash ON competition_results(instance_hash);

-- Instance metadata from GBD
CREATE TABLE IF NOT EXISTS instances (
    hash            TEXT PRIMARY KEY,
    filename        TEXT,
    family          TEXT,
    track           TEXT,
    result          TEXT
);

CREATE INDEX IF NOT EXISTS idx_instances_family ON instances(family);

-- Instance file mapping (our hash -> filename)
CREATE TABLE IF NOT EXISTS instance_files (
    hash            TEXT NOT NULL,
    value           TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_if_hash ON instance_files(hash);

-- Family parameters from Optuna tuning
CREATE TABLE IF NOT EXISTS family_params (
    family          TEXT PRIMARY KEY,
    beta            DOUBLE PRECISION,
    gamma           DOUBLE PRECISION,
    delta           DOUBLE PRECISION,
    epsilon         DOUBLE PRECISION,
    alpha_initial   DOUBLE PRECISION,
    alpha_up_mult   DOUBLE PRECISION,
    alpha_down_mult DOUBLE PRECISION,
    alpha_interval  DOUBLE PRECISION,
    zeta            DOUBLE PRECISION,
    method          TEXT,
    restart_mode    TEXT,
    preprocess      BOOLEAN,
    source          TEXT,
    par2_score      DOUBLE PRECISION,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Best times view
CREATE OR REPLACE VIEW best_times AS
SELECT DISTINCT ON (r.instance_hash)
    r.instance_hash,
    r.time_s AS best_time,
    ru.solver_version,
    ru.git_commit,
    r.run_id
FROM results r
JOIN runs ru USING(run_id)
WHERE r.status = 'SATISFIABLE'
ORDER BY r.instance_hash, r.time_s ASC;

-- Record schema version
INSERT INTO schema_version (version, description)
VALUES (1, 'Initial schema: runs, results, competition_results, instances, instance_files, family_params')
ON CONFLICT (version) DO NOTHING;
"""


def create_schema(db_url):
    conn = psycopg2.connect(db_url)
    conn.autocommit = True
    cur = conn.cursor()
    cur.execute(SCHEMA_SQL)
    cur.execute("SELECT version, description FROM schema_version ORDER BY version")
    for version, desc in cur.fetchall():
        print(f"  Schema v{version}: {desc}")
    conn.close()


def main():
    parser = argparse.ArgumentParser(description="Initialize benchmarks PostgreSQL schema")
    parser.add_argument("--db-url", help="PostgreSQL connection URL")
    args = parser.parse_args()

    db_url = args.db_url or get_db_url()
    print(f"Connecting to: {db_url.split('@')[1] if '@' in db_url else db_url}")
    create_schema(db_url)
    print("Schema created successfully.")


if __name__ == "__main__":
    main()
