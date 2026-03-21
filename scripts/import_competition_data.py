#!/usr/bin/env python3
"""
Import SAT Competition reference results into benchmarks.db.

Supports importing from:
1. SAT Competition detailed results CSV files
2. The al-for-sat-solver-benchmarking-data repository (Anniversary Track)
3. Manual CSV format: instance_hash,competition,solver,status,time_s

Usage:
    # Import from Anniversary Track SQLite (recommended first import)
    python3 scripts/import_competition_data.py --anni-db path/to/base.db --anni-csv path/to/anni-seq.csv

    # Import from SAT Competition detailed results
    python3 scripts/import_competition_data.py --csv path/to/detailed_results.csv --competition sc2024

    # Show what's in the DB
    python3 scripts/import_competition_data.py --status
"""

import argparse
import csv
import sqlite3
import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent
BENCHMARKS_DB = PROJECT_ROOT / "benchmarks.db"


def show_status():
    """Show current competition data in benchmarks.db."""
    if not BENCHMARKS_DB.exists():
        print(f"Error: {BENCHMARKS_DB} not found. Run init_benchmarks_db.py first.")
        return

    conn = sqlite3.connect(str(BENCHMARKS_DB))
    cursor = conn.cursor()

    cursor.execute("SELECT COUNT(*) FROM competition_results")
    total = cursor.fetchone()[0]

    if total == 0:
        print("No competition data imported yet.")
        print()
        print("To get started:")
        print("  1. Clone: git clone https://github.com/mathefuchs/al-for-sat-solver-benchmarking-data")
        print("  2. Run:   python3 scripts/import_competition_data.py \\")
        print("              --anni-csv <repo>/gbd-data/anni-seq.csv \\")
        print("              --anni-db <repo>/gbd-data/base.db")
        conn.close()
        return

    cursor.execute("""
        SELECT competition, COUNT(DISTINCT solver) as solvers,
               COUNT(DISTINCT instance_hash) as instances,
               COUNT(*) as total_results
        FROM competition_results
        GROUP BY competition
        ORDER BY competition
    """)
    rows = cursor.fetchall()

    print(f"Competition data in {BENCHMARKS_DB}:")
    print(f"  Total results: {total}")
    print()
    print(f"  {'Competition':<20} {'Solvers':<10} {'Instances':<12} {'Results':<10}")
    print(f"  {'-'*52}")
    for comp, solvers, instances, results in rows:
        print(f"  {comp:<20} {solvers:<10} {instances:<12} {results:<10}")

    # Show top solvers
    cursor.execute("""
        SELECT solver,
               SUM(CASE WHEN status = 'SAT' OR status = 'SATISFIABLE' THEN 1 ELSE 0 END) as sat_count,
               COUNT(*) as total,
               ROUND(AVG(CASE WHEN time_s IS NOT NULL AND time_s > 0 THEN time_s END), 2) as avg_time
        FROM competition_results
        GROUP BY solver
        ORDER BY sat_count DESC
        LIMIT 10
    """)
    rows = cursor.fetchall()

    if rows:
        print()
        print(f"  Top solvers by SAT count:")
        print(f"  {'Solver':<30} {'SAT':<8} {'Total':<8} {'Avg Time':<10}")
        print(f"  {'-'*56}")
        for solver, sat, total, avg in rows:
            avg_str = f"{avg:.2f}s" if avg else "N/A"
            print(f"  {solver:<30} {sat:<8} {total:<8} {avg_str:<10}")

    conn.close()


def import_generic_csv(csv_path, competition, delimiter=","):
    """Import a generic CSV with columns: instance/hash, solver, status, time."""
    if not BENCHMARKS_DB.exists():
        print(f"Error: {BENCHMARKS_DB} not found. Run init_benchmarks_db.py first.")
        return

    conn = sqlite3.connect(str(BENCHMARKS_DB))
    cursor = conn.cursor()

    imported = 0
    skipped = 0

    with open(csv_path) as f:
        reader = csv.DictReader(f, delimiter=delimiter)
        headers = reader.fieldnames

        # Try to identify columns
        hash_col = None
        for candidate in ["hash", "instance_hash", "instance", "benchmark"]:
            if candidate in headers:
                hash_col = candidate
                break

        solver_col = None
        for candidate in ["solver", "solver_name"]:
            if candidate in headers:
                solver_col = candidate
                break

        status_col = None
        for candidate in ["status", "result", "answer"]:
            if candidate in headers:
                status_col = candidate
                break

        time_col = None
        for candidate in ["time_s", "time", "runtime", "cpu_time", "wallclock"]:
            if candidate in headers:
                time_col = candidate
                break

        if not hash_col or not solver_col:
            print(f"Error: Could not identify required columns in {csv_path}")
            print(f"  Available columns: {headers}")
            print(f"  Need: instance/hash column + solver column")
            conn.close()
            return

        print(f"Importing from {csv_path}")
        print(f"  Columns: hash={hash_col}, solver={solver_col}, status={status_col}, time={time_col}")

        for row in reader:
            instance_hash = row[hash_col]
            solver = row[solver_col]
            status = row.get(status_col, "UNKNOWN") if status_col else "UNKNOWN"
            time_s = None
            if time_col and row.get(time_col):
                try:
                    time_s = float(row[time_col])
                except (ValueError, TypeError):
                    pass

            try:
                cursor.execute("""
                    INSERT OR REPLACE INTO competition_results
                    (instance_hash, competition, solver, status, time_s)
                    VALUES (?, ?, ?, ?, ?)
                """, (instance_hash, competition, solver, status, time_s))
                imported += 1
            except sqlite3.Error:
                skipped += 1

    conn.commit()
    conn.close()
    print(f"  Imported: {imported}, Skipped: {skipped}")


def import_anniversary_data(anni_csv, anni_db=None):
    """Import Anniversary Track data from the AL-for-SAT-Solver-Benchmarking repo.

    The repo structure:
    - gbd-data/anni-seq.csv: solver runtimes (wide format: hash, solver1, solver2, ...)
    - gbd-data/base.db: SQLite with instance features and metadata
    """
    if not BENCHMARKS_DB.exists():
        print(f"Error: {BENCHMARKS_DB} not found. Run init_benchmarks_db.py first.")
        return

    anni_csv = Path(anni_csv)
    if not anni_csv.exists():
        print(f"Error: {anni_csv} not found.")
        print("Clone: git clone https://github.com/mathefuchs/al-for-sat-solver-benchmarking-data")
        return

    conn = sqlite3.connect(str(BENCHMARKS_DB))
    cursor = conn.cursor()

    # Import runtime data from CSV (wide format → long format)
    print(f"Importing Anniversary Track runtimes from {anni_csv}")
    imported = 0
    competition = "anni2022"

    with open(anni_csv) as f:
        reader = csv.DictReader(f)
        solvers = [col for col in reader.fieldnames if col != "hash"]
        print(f"  Found {len(solvers)} solvers: {', '.join(solvers[:5])}...")

        for row in reader:
            instance_hash = row["hash"]
            for solver in solvers:
                time_str = row.get(solver, "").strip()
                if not time_str:
                    continue

                try:
                    time_s = float(time_str)
                except ValueError:
                    continue

                # Determine status: if time < 5000 (timeout), it was solved
                status = "SAT" if time_s < 5000.0 else "TIMEOUT"

                cursor.execute("""
                    INSERT OR REPLACE INTO competition_results
                    (instance_hash, competition, solver, status, time_s)
                    VALUES (?, ?, ?, ?, ?)
                """, (instance_hash, competition, solver, status, time_s))
                imported += 1

    conn.commit()

    # Import instance features from base.db if provided
    if anni_db:
        anni_db = Path(anni_db)
        if anni_db.exists():
            print(f"Importing instance features from {anni_db}")
            cursor.execute(f"ATTACH DATABASE ? AS anni", (str(anni_db),))

            # Check what tables are available
            cursor.execute("SELECT name FROM anni.sqlite_master WHERE type='table'")
            tables = [r[0] for r in cursor.fetchall()]
            print(f"  Available tables: {tables}")

            # Try to import features
            feature_count = 0
            for table in tables:
                cursor.execute(f"PRAGMA anni.table_info({table})")
                cols = [r[1] for r in cursor.fetchall()]
                if "hash" in cols:
                    # Check for useful feature columns
                    feature_cols = [c for c in cols if c not in ("hash", "rowid")]
                    if feature_cols:
                        print(f"  Table '{table}': {len(feature_cols)} feature columns")

            cursor.execute("DETACH DATABASE anni")
            conn.commit()

    cursor.execute("SELECT COUNT(*) FROM competition_results WHERE competition = ?",
                    (competition,))
    total = cursor.fetchone()[0]
    cursor.execute("SELECT COUNT(DISTINCT instance_hash) FROM competition_results WHERE competition = ?",
                    (competition,))
    instances = cursor.fetchone()[0]
    cursor.execute("SELECT COUNT(DISTINCT solver) FROM competition_results WHERE competition = ?",
                    (competition,))
    solver_count = cursor.fetchone()[0]

    conn.close()
    print(f"  Imported: {imported} results ({instances} instances, {solver_count} solvers)")
    print(f"  Total competition_results: {total}")


def main():
    parser = argparse.ArgumentParser(
        description="Import SAT Competition reference data into benchmarks.db")
    parser.add_argument("--status", action="store_true",
                        help="Show current competition data status")
    parser.add_argument("--csv", type=Path,
                        help="Import from a generic CSV file")
    parser.add_argument("--competition", default="unknown",
                        help="Competition name for CSV import (e.g., sc2024)")
    parser.add_argument("--delimiter", default=",",
                        help="CSV delimiter (default: comma)")
    parser.add_argument("--anni-csv", type=Path,
                        help="Path to anni-seq.csv from AL-for-SAT repo")
    parser.add_argument("--anni-db", type=Path,
                        help="Path to base.db from AL-for-SAT repo (optional, for features)")

    args = parser.parse_args()

    if args.status:
        show_status()
        return

    if args.anni_csv:
        import_anniversary_data(args.anni_csv, args.anni_db)
        return

    if args.csv:
        import_generic_csv(args.csv, args.competition, args.delimiter)
        return

    # No action specified — show help
    parser.print_help()
    print()
    print("Quick start:")
    print("  python3 scripts/import_competition_data.py --status")


if __name__ == "__main__":
    main()
