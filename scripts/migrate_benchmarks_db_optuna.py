#!/usr/bin/env python3
"""
Idempotent schema migration for Optuna tuning columns.

Adds columns needed by the Optuna experiment framework (Phase 1):
  - runs: optuna_study, optuna_trial_number
  - results: xl_decay, restart_noise, alpha_initial, alpha_up_mult,
             alpha_down_mult, alpha_interval, restart_mode, strategy_used,
             preprocess_enabled

Safe to run multiple times — checks column existence before ALTER TABLE.

Usage:
    python3 scripts/migrate_benchmarks_db_optuna.py
    python3 scripts/migrate_benchmarks_db_optuna.py --db /path/to/benchmarks.db
    python3 scripts/migrate_benchmarks_db_optuna.py --dry-run
"""

import argparse
import sqlite3
import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent
DEFAULT_DB = PROJECT_ROOT / "benchmarks.db"

# --- Optuna column additions ---

RUNS_OPTUNA_COLUMNS = [
    ("optuna_study", "TEXT"),
    ("optuna_trial_number", "INTEGER"),
]

RESULTS_OPTUNA_COLUMNS = [
    ("xl_decay", "REAL"),
    ("restart_noise", "REAL"),
    ("alpha_initial", "REAL"),
    ("alpha_up_mult", "REAL"),
    ("alpha_down_mult", "REAL"),
    ("alpha_interval", "REAL"),
    ("restart_mode", "TEXT"),
    ("strategy_used", "TEXT"),
    ("preprocess_enabled", "INTEGER"),
]


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


def migrate(db_path, dry_run=False):
    """Run the Optuna schema migration."""
    if not db_path.exists():
        print(f"Error: database not found at {db_path}")
        sys.exit(1)

    print(f"Migrating {db_path} (Optuna columns)")
    if dry_run:
        print("(DRY RUN — no changes will be made)\n")

    conn = sqlite3.connect(str(db_path))
    cursor = conn.cursor()

    # Verify target tables exist
    for table in ("runs", "results"):
        if not table_exists(cursor, table):
            print(f"Error: {table} table not found — run init_benchmarks_db.py first")
            conn.close()
            sys.exit(1)

    # 1. Add Optuna columns to runs
    print("\n[1/2] Adding Optuna columns to runs table...")
    runs_added = add_columns(cursor, "runs", RUNS_OPTUNA_COLUMNS, dry_run)

    # 2. Add Optuna columns to results
    print("\n[2/2] Adding Optuna columns to results table...")
    results_added = add_columns(cursor, "results", RESULTS_OPTUNA_COLUMNS, dry_run)

    total = runs_added + results_added
    if not dry_run:
        conn.commit()
        if total > 0:
            print(f"\nMigration complete. Added {total} columns.")
        else:
            print("\nNo changes needed — all columns already exist.")
    else:
        if total > 0:
            print(f"\nDry run complete. Would add {total} columns.")
        else:
            print("\nDry run complete. All columns already exist.")

    conn.close()


def main():
    parser = argparse.ArgumentParser(
        description="Migrate benchmarks.db schema for Optuna columns (idempotent)"
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
