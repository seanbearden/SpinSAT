#!/usr/bin/env python3
"""
One-time migration for existing benchmarks.db.

Adds new columns, creates new tables, and backfills data for the
schema changes specified in ss-34v:
  - runs: restart_strategy, preprocessing, cli_command, tag_purpose,
          tag_instance_set, tag_config
  - results: peak_xl_max, final_dt, wall_clock_s, cpu_time_s, num_vars, num_clauses
  - New table: instance_year_track (materialized from instance_tracks)
  - New table: competition_best (best solver per benchmarked instance)

Usage:
    python3 scripts/migrate_benchmarks_db.py
    python3 scripts/migrate_benchmarks_db.py --db /path/to/benchmarks.db
    python3 scripts/migrate_benchmarks_db.py --dry-run
"""

import argparse
import re
import sqlite3
import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent
DEFAULT_DB = PROJECT_ROOT / "benchmarks.db"

# --- Column additions ---

RUNS_NEW_COLUMNS = [
    ("restart_strategy", "TEXT"),
    ("preprocessing", "TEXT"),
    ("cli_command", "TEXT"),
    ("tag_purpose", "TEXT"),
    ("tag_instance_set", "TEXT"),
    ("tag_config", "TEXT"),
]

RESULTS_NEW_COLUMNS = [
    ("peak_xl_max", "REAL"),
    ("final_dt", "REAL"),
    ("wall_clock_s", "REAL"),
    ("cpu_time_s", "REAL"),
    ("num_vars", "INTEGER"),
    ("num_clauses", "INTEGER"),
]

# --- Tag parsing ---

# Known tag_purpose patterns
PURPOSE_PATTERNS = {
    "paper-verification": re.compile(
        r"paper|verify|verification|barthel|komb|qhid", re.IGNORECASE
    ),
    "competition-benchmark": re.compile(
        r"anni|competition|comp|sat20\d\d", re.IGNORECASE
    ),
    "regression-test": re.compile(r"regress|ci|check", re.IGNORECASE),
    "development": re.compile(r"dev|test|debug|tune|experiment", re.IGNORECASE),
}

# Known tag_instance_set patterns
INSTANCE_SET_PATTERNS = {
    "barthel": re.compile(r"barthel", re.IGNORECASE),
    "komb": re.compile(r"komb", re.IGNORECASE),
    "qhid": re.compile(r"qhid", re.IGNORECASE),
    "anni2022": re.compile(r"anni.*2022|2022.*anni", re.IGNORECASE),
    "sat2025-main": re.compile(r"sat2025|2025.*main", re.IGNORECASE),
    "uniform-random": re.compile(r"uniform|random|uf\d+", re.IGNORECASE),
}

# Known tag_config patterns
CONFIG_PATTERNS = {
    "cycling": re.compile(r"cycl", re.IGNORECASE),
    "smart-restart": re.compile(r"smart.?restart", re.IGNORECASE),
    "no-preprocess": re.compile(r"no.?preprocess", re.IGNORECASE),
    "default": re.compile(r"default|baseline", re.IGNORECASE),
}

# --- Track parsing ---

# Parse SAT competition track values like "main_2022", "random_2018", "anniversary_2022"
TRACK_PATTERN = re.compile(r"^(\w+?)_(\d{4})$")


def column_exists(cursor, table, column):
    """Check if a column already exists in a table."""
    cursor.execute(f"PRAGMA table_info({table})")
    return any(row[1] == column for row in cursor.fetchall())


def table_exists(cursor, table):
    """Check if a table exists."""
    cursor.execute(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
        (table,),
    )
    return cursor.fetchone()[0] > 0


def add_columns(cursor, table, columns, dry_run=False):
    """Add new columns to a table if they don't exist."""
    added = 0
    for col_name, col_type in columns:
        if not column_exists(cursor, table, col_name):
            stmt = f"ALTER TABLE {table} ADD COLUMN {col_name} {col_type}"
            if dry_run:
                print(f"  [DRY RUN] {stmt}")
            else:
                cursor.execute(stmt)
                print(f"  Added {table}.{col_name} ({col_type})")
            added += 1
        else:
            print(f"  {table}.{col_name} already exists, skipping")
    return added


def classify_tag(tag, patterns):
    """Match a tag string against a dict of pattern->label."""
    if not tag:
        return None
    for label, pattern in patterns.items():
        if pattern.search(tag):
            return label
    return None


def backfill_structured_tags(cursor, dry_run=False):
    """Parse existing tag strings into structured tag_* columns."""
    cursor.execute("SELECT run_id, tag FROM runs WHERE tag IS NOT NULL AND tag != ''")
    rows = cursor.fetchall()
    updated = 0
    for run_id, tag in rows:
        purpose = classify_tag(tag, PURPOSE_PATTERNS)
        instance_set = classify_tag(tag, INSTANCE_SET_PATTERNS)
        config = classify_tag(tag, CONFIG_PATTERNS)

        if purpose or instance_set or config:
            if dry_run:
                print(
                    f"  [DRY RUN] tag '{tag}' -> purpose={purpose}, "
                    f"instance_set={instance_set}, config={config}"
                )
            else:
                cursor.execute(
                    """UPDATE runs
                       SET tag_purpose = COALESCE(tag_purpose, ?),
                           tag_instance_set = COALESCE(tag_instance_set, ?),
                           tag_config = COALESCE(tag_config, ?)
                       WHERE run_id = ?""",
                    (purpose, instance_set, config, run_id),
                )
            updated += 1
    return updated


def backfill_num_vars_clauses(cursor, dry_run=False):
    """Backfill results.num_vars and results.num_clauses from instance_features."""
    if not table_exists(cursor, "instance_features"):
        print("  instance_features table not found, skipping backfill")
        return 0

    if dry_run:
        cursor.execute(
            """SELECT COUNT(*) FROM results r
               JOIN instance_features f ON r.instance_hash = f.instance_hash
               WHERE r.num_vars IS NULL AND f.num_vars IS NOT NULL"""
        )
        count = cursor.fetchone()[0]
        print(f"  [DRY RUN] Would backfill num_vars/num_clauses for {count} rows")
        return count

    cursor.execute(
        """UPDATE results
           SET num_vars = (
               SELECT f.num_vars FROM instance_features f
               WHERE f.instance_hash = results.instance_hash
           ),
           num_clauses = (
               SELECT f.num_clauses FROM instance_features f
               WHERE f.instance_hash = results.instance_hash
           )
           WHERE num_vars IS NULL
             AND instance_hash IN (
               SELECT instance_hash FROM instance_features
               WHERE num_vars IS NOT NULL
             )"""
    )
    updated = cursor.rowcount
    return updated


def populate_instance_year_track(cursor, dry_run=False):
    """Materialize instance_year_track from instance_tracks."""
    if not table_exists(cursor, "instance_tracks"):
        print("  instance_tracks table not found, skipping")
        return 0

    # Create table if not exists
    if not dry_run:
        cursor.execute(
            """CREATE TABLE IF NOT EXISTS instance_year_track (
                   hash TEXT PRIMARY KEY,
                   year INTEGER,
                   track_type TEXT
               )"""
        )

    # Parse track values (e.g., "main_2022" -> year=2022, track_type="main")
    cursor.execute("SELECT DISTINCT hash, value FROM instance_tracks")
    rows = cursor.fetchall()

    # For instances with multiple tracks, keep the most recent year
    best = {}
    for hash_val, track_value in rows:
        m = TRACK_PATTERN.match(track_value)
        if m:
            track_type = m.group(1)
            year = int(m.group(2))
            if hash_val not in best or year > best[hash_val][1]:
                best[hash_val] = (hash_val, year, track_type)
    parsed = list(best.values())

    if dry_run:
        print(
            f"  [DRY RUN] Would populate instance_year_track with {len(parsed)} rows "
            f"from {len(rows)} track entries"
        )
        return len(parsed)

    # Clear and repopulate
    cursor.execute("DELETE FROM instance_year_track")
    cursor.executemany(
        "INSERT OR REPLACE INTO instance_year_track (hash, year, track_type) VALUES (?, ?, ?)",
        parsed,
    )

    # Add indexes
    cursor.execute(
        "CREATE INDEX IF NOT EXISTS idx_instance_year_track_year ON instance_year_track(year)"
    )
    cursor.execute(
        "CREATE INDEX IF NOT EXISTS idx_instance_year_track_type ON instance_year_track(track_type)"
    )

    return len(parsed)


def populate_competition_best(cursor, dry_run=False):
    """Populate competition_best with best solver time per benchmarked instance."""
    if not table_exists(cursor, "competition_results"):
        print("  competition_results table not found, skipping")
        return 0

    # Create table if not exists
    if not dry_run:
        cursor.execute(
            """CREATE TABLE IF NOT EXISTS competition_best (
                   instance_hash TEXT PRIMARY KEY,
                   best_solver TEXT,
                   best_time_s REAL,
                   competition TEXT,
                   timeout_s INTEGER
               )"""
        )

    # Only include instances that appear in our benchmark results
    query = """
        SELECT cr.instance_hash, cr.solver, cr.time_s, cr.competition
        FROM competition_results cr
        WHERE cr.status = 'SAT'
          AND cr.instance_hash IN (SELECT DISTINCT instance_hash FROM results)
        ORDER BY cr.instance_hash, cr.time_s ASC
    """

    if dry_run:
        cursor.execute(
            """SELECT COUNT(DISTINCT cr.instance_hash)
               FROM competition_results cr
               WHERE cr.status = 'SAT'
                 AND cr.instance_hash IN (SELECT DISTINCT instance_hash FROM results)"""
        )
        count = cursor.fetchone()[0]
        print(f"  [DRY RUN] Would populate competition_best for {count} instances")
        return count

    cursor.execute(query)
    rows = cursor.fetchall()

    # Keep best (minimum time) per instance
    best = {}
    for instance_hash, solver, time_s, competition in rows:
        if instance_hash not in best or (time_s is not None and time_s < best[instance_hash][1]):
            best[instance_hash] = (solver, time_s, competition)

    # Clear and repopulate
    cursor.execute("DELETE FROM competition_best")
    for instance_hash, (solver, time_s, competition) in best.items():
        cursor.execute(
            """INSERT INTO competition_best
               (instance_hash, best_solver, best_time_s, competition, timeout_s)
               VALUES (?, ?, ?, ?, ?)""",
            (instance_hash, solver, time_s, competition, 5000),
        )

    # Add index
    cursor.execute(
        "CREATE INDEX IF NOT EXISTS idx_competition_best_hash ON competition_best(instance_hash)"
    )

    return len(best)


def migrate(db_path, dry_run=False):
    """Run the full migration."""
    if not db_path.exists():
        print(f"Error: database not found at {db_path}")
        sys.exit(1)

    print(f"Migrating {db_path}")
    if dry_run:
        print("(DRY RUN — no changes will be made)\n")

    conn = sqlite3.connect(str(db_path))
    cursor = conn.cursor()

    # 1. Add new columns to runs
    print("\n[1/6] Adding columns to runs table...")
    add_columns(cursor, "runs", RUNS_NEW_COLUMNS, dry_run)

    # 2. Add new columns to results
    print("\n[2/6] Adding columns to results table...")
    add_columns(cursor, "results", RESULTS_NEW_COLUMNS, dry_run)

    # 3. Backfill structured tags from existing tag strings
    print("\n[3/6] Backfilling structured tags...")
    tag_count = backfill_structured_tags(cursor, dry_run)
    print(f"  Updated {tag_count} runs with structured tags")

    # 4. Backfill num_vars/num_clauses from instance_features
    print("\n[4/6] Backfilling num_vars/num_clauses in results...")
    feat_count = backfill_num_vars_clauses(cursor, dry_run)
    print(f"  Backfilled {feat_count} result rows")

    # 5. Populate instance_year_track
    print("\n[5/6] Populating instance_year_track...")
    track_count = populate_instance_year_track(cursor, dry_run)
    print(f"  Populated {track_count} rows")

    # 6. Populate competition_best
    print("\n[6/6] Populating competition_best...")
    best_count = populate_competition_best(cursor, dry_run)
    print(f"  Populated {best_count} rows")

    if not dry_run:
        conn.commit()
        print("\nMigration complete. All changes committed.")
    else:
        print("\nDry run complete. No changes made.")

    conn.close()


def main():
    parser = argparse.ArgumentParser(
        description="Migrate benchmarks.db schema (one-time)"
    )
    parser.add_argument(
        "--db",
        type=Path,
        default=DEFAULT_DB,
        help=f"Path to benchmarks.db (default: {DEFAULT_DB})",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would be done without making changes",
    )
    args = parser.parse_args()
    migrate(args.db, args.dry_run)


if __name__ == "__main__":
    main()
