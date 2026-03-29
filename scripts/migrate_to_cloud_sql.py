#!/usr/bin/env python3
"""Migrate benchmarks.db to Cloud SQL using COPY with streaming."""
import sqlite3, psycopg2, io, os, sys

PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SQLITE_DB = os.path.join(PROJECT_ROOT, "benchmarks.db")
PW_FILE = os.path.join(PROJECT_ROOT, "optuna_studies", ".db-password-spinsat-optuna")

pw = open(PW_FILE).read().strip()
pg_conn = psycopg2.connect(
    host='34.57.20.164', dbname='spinsat_benchmarks',
    user='benchmarks', password=pw,
    options='-c statement_timeout=0'  # no timeout
)
sqlite_conn = sqlite3.connect(SQLITE_DB)
scur = sqlite_conn.cursor()
pcur = pg_conn.cursor()


def stream_copy(table, query, pg_columns):
    """Stream rows from SQLite to PG via COPY protocol."""
    scur.execute(query)
    buf = io.StringIO()
    count = 0
    for row in scur:
        fields = []
        for val in row:
            if val is None:
                fields.append('\\N')
            else:
                s = str(val).replace('\\', '\\\\').replace('\t', '\\t').replace('\n', '\\n')
                fields.append(s)
        buf.write('\t'.join(fields) + '\n')
        count += 1
    buf.seek(0)
    pcur.execute(f"TRUNCATE {table} CASCADE")
    pcur.copy_from(buf, table, columns=pg_columns, null='\\N')
    pg_conn.commit()
    print(f"  {table}: {count} rows", flush=True)


print("Migrating to Cloud SQL...", flush=True)

# Order matters: runs before results (FK)
stream_copy('runs',
    "SELECT run_id, solver_version, git_commit, "
    "CASE WHEN git_dirty THEN 'true' ELSE 'false' END, "
    "integration_method, strategy, timestamp, timeout_s, "
    "hardware, rust_version, tag, notes FROM runs",
    ('run_id', 'solver_version', 'git_commit', 'git_dirty',
     'integration_method', 'strategy', 'timestamp', 'timeout_s',
     'hardware', 'rust_version', 'tag', 'notes'))

stream_copy('results',
    "SELECT run_id, instance_hash, status, time_s, steps, restarts, "
    "verified, seed, zeta, alpha, beta, gamma, delta, epsilon, "
    "dt_min, dt_max, peak_xl_max, final_dt, wall_clock_s, cpu_time_s, "
    "num_vars, num_clauses FROM results",
    ('run_id', 'instance_hash', 'status', 'time_s', 'steps', 'restarts',
     'verified', 'seed', 'zeta', 'alpha', 'beta', 'gamma', 'delta', 'epsilon',
     'dt_min', 'dt_max', 'peak_xl_max', 'final_dt', 'wall_clock_s', 'cpu_time_s',
     'num_vars', 'num_clauses'))

stream_copy('competition_results',
    "SELECT instance_hash, competition, solver, status, time_s "
    "FROM competition_results",
    ('instance_hash', 'competition', 'solver', 'status', 'time_s'))

stream_copy('instance_files',
    "SELECT DISTINCT hash, value FROM instance_files",
    ('hash', 'value'))

stream_copy('instances',
    "SELECT hash, filename, family, track, result FROM instances",
    ('hash', 'filename', 'family', 'track', 'result'))

pg_conn.close()
sqlite_conn.close()
print("Done!", flush=True)
