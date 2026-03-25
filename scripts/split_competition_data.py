#!/usr/bin/env python3
"""
Split competition_results out of benchmarks.db into a separate archive DB.

Keeps only competition_best (one row per benchmarked instance) in benchmarks.db.
Full competition data goes to competition_archive.db for upload to GitHub Releases.

Usage:
    python3 scripts/split_competition_data.py
    python3 scripts/split_competition_data.py --dry-run
    python3 scripts/split_competition_data.py --keep-original  # don't drop competition_results
"""

import argparse
import sqlite3
import shutil
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent
BENCHMARKS_DB = PROJECT_ROOT / "benchmarks.db"
ARCHIVE_DB = PROJECT_ROOT / "competition_archive.db"


def split_competition_data(dry_run=False, keep_original=False):
    """Extract competition_results to archive DB, keep competition_best in main DB."""
    if not BENCHMARKS_DB.exists():
        print(f"Error: {BENCHMARKS_DB} not found")
        return

    conn = sqlite3.connect(str(BENCHMARKS_DB))
    cursor = conn.cursor()

    # Check current state
    cursor.execute("SELECT COUNT(*) FROM competition_results")
    comp_count = cursor.fetchone()[0]

    cursor.execute("SELECT COUNT(*) FROM competition_best")
    best_count = cursor.fetchone()[0]

    print(f"Current state:")
    print(f"  competition_results: {comp_count:,} rows")
    print(f"  competition_best:    {best_count:,} rows")

    if comp_count == 0:
        print("\nNo competition_results to archive. Nothing to do.")
        conn.close()
        return

    if dry_run:
        # Show what would happen
        cursor.execute("""
            SELECT COUNT(DISTINCT instance_hash) FROM competition_results
            WHERE instance_hash IN (SELECT DISTINCT instance_hash FROM results)
        """)
        benchmarked = cursor.fetchone()[0]

        cursor.execute("SELECT COUNT(DISTINCT solver) FROM competition_results")
        solvers = cursor.fetchone()[0]

        cursor.execute("SELECT COUNT(DISTINCT competition) FROM competition_results")
        competitions = cursor.fetchone()[0]

        # Estimate size savings
        cursor.execute("SELECT page_count * page_size FROM pragma_page_count, pragma_page_size")
        db_size = cursor.fetchone()[0]

        print(f"\nWould archive:")
        print(f"  {comp_count:,} competition results ({solvers} solvers, {competitions} competitions)")
        print(f"  Benchmarked instances with comp data: {benchmarked}")
        print(f"  competition_best would retain: {benchmarked} rows")
        print(f"\nCurrent DB size: {db_size / 1024 / 1024:.1f} MB")
        print(f"Expected reduction: ~30-50% (competition_results is the largest table)")
        conn.close()
        return

    # Step 1: Create archive DB
    print(f"\nCreating archive: {ARCHIVE_DB}")
    if ARCHIVE_DB.exists():
        ARCHIVE_DB.unlink()

    archive_conn = sqlite3.connect(str(ARCHIVE_DB))
    archive_cursor = archive_conn.cursor()

    archive_cursor.execute("""
        CREATE TABLE competition_results (
            instance_hash TEXT NOT NULL,
            competition TEXT NOT NULL,
            solver TEXT NOT NULL,
            status TEXT,
            time_s REAL,
            PRIMARY KEY (instance_hash, competition, solver)
        )
    """)
    archive_cursor.execute("CREATE INDEX idx_comp_hash ON competition_results(instance_hash)")
    archive_cursor.execute("CREATE INDEX idx_comp_solver ON competition_results(solver)")

    # Step 2: Copy data to archive
    cursor.execute("SELECT * FROM competition_results")
    rows = cursor.fetchall()
    archive_cursor.executemany(
        "INSERT INTO competition_results VALUES (?, ?, ?, ?, ?)",
        rows
    )
    archive_conn.commit()

    archive_cursor.execute("SELECT COUNT(*) FROM competition_results")
    archived = archive_cursor.fetchone()[0]
    archive_conn.close()

    archive_size = ARCHIVE_DB.stat().st_size / 1024 / 1024
    print(f"  Archived {archived:,} rows ({archive_size:.1f} MB)")

    # Step 3: Ensure competition_best is populated
    cursor.execute("DELETE FROM competition_best")
    cursor.execute("""
        INSERT INTO competition_best
            (instance_hash, best_solver, best_time_s, competition, timeout_s)
        SELECT
            cr.instance_hash,
            cr.solver,
            MIN(cr.time_s),
            cr.competition,
            5000
        FROM competition_results cr
        WHERE cr.status IN ('SAT', 'SATISFIABLE')
          AND cr.instance_hash IN (SELECT DISTINCT instance_hash FROM results)
        GROUP BY cr.instance_hash
    """)
    best_updated = cursor.rowcount
    print(f"  competition_best: {best_updated} rows (benchmarked instances only)")

    # Step 4: Drop competition_results from main DB
    if not keep_original:
        cursor.execute("DROP TABLE IF EXISTS competition_results")
        # Recreate empty table for import script compatibility
        cursor.execute("""
            CREATE TABLE IF NOT EXISTS competition_results (
                instance_hash TEXT NOT NULL,
                competition TEXT NOT NULL,
                solver TEXT NOT NULL,
                status TEXT,
                time_s REAL,
                PRIMARY KEY (instance_hash, competition, solver)
            )
        """)
        cursor.execute("CREATE INDEX IF NOT EXISTS idx_competition_hash ON competition_results(instance_hash)")
        cursor.execute("CREATE INDEX IF NOT EXISTS idx_competition_solver ON competition_results(solver)")
        print(f"  Dropped competition_results from main DB (empty table preserved for compat)")

    conn.commit()

    # VACUUM to reclaim space
    print("  VACUUMing main DB...")
    conn.execute("VACUUM")
    conn.close()

    main_size = BENCHMARKS_DB.stat().st_size / 1024 / 1024
    print(f"\nDone!")
    print(f"  {BENCHMARKS_DB.name}: {main_size:.1f} MB")
    print(f"  {ARCHIVE_DB.name}: {archive_size:.1f} MB")
    print(f"\nNext steps:")
    print(f"  gh release upload <tag> {ARCHIVE_DB} --clobber")


def main():
    parser = argparse.ArgumentParser(description="Split competition data into archive DB")
    parser.add_argument("--dry-run", action="store_true",
                        help="Show what would happen without changing anything")
    parser.add_argument("--keep-original", action="store_true",
                        help="Keep competition_results in main DB (just create archive)")
    args = parser.parse_args()

    split_competition_data(dry_run=args.dry_run, keep_original=args.keep_original)


if __name__ == "__main__":
    main()
