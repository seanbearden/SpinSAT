#!/usr/bin/env python3
"""
Download SAT competition instances from GBD (Global Benchmark Database).

Uses the instance hash to download from https://benchmark-database.de/file/<hash>

Usage:
    python3 scripts/download_competition_instances.py --track random_2002 --limit 20 --result sat
    python3 scripts/download_competition_instances.py --hashes hash1 hash2 hash3
"""

import argparse
import lzma
import os
import sqlite3
import subprocess
import sys
import urllib.request
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent
BENCHMARKS_DB = PROJECT_ROOT / "benchmarks.db"
COMPETITION_DIR = PROJECT_ROOT / "benchmarks" / "competition"
GBD_BASE_URL = "https://benchmark-database.de/file"


def download_instance(hash_id, filename, output_dir):
    """Download and decompress a single instance from GBD."""
    output_dir = Path(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    # Strip .xz extension for output filename
    cnf_name = filename.replace(".xz", "") if filename.endswith(".xz") else filename
    output_path = output_dir / cnf_name

    if output_path.exists():
        return str(output_path), "cached"

    url = f"{GBD_BASE_URL}/{hash_id}"
    try:
        req = urllib.request.Request(url)
        with urllib.request.urlopen(req, timeout=60) as resp:
            data = resp.read()

        # Decompress if xz
        if filename.endswith(".xz"):
            data = lzma.decompress(data)

        output_path.write_bytes(data)
        return str(output_path), "downloaded"
    except Exception as e:
        return None, str(e)


def get_instances_by_track(track, result_filter=None, limit=20):
    """Query benchmarks.db for instances in a track."""
    conn = sqlite3.connect(str(BENCHMARKS_DB))
    cursor = conn.cursor()

    query = """
        SELECT DISTINCT i.hash, if2.value as filename, i.family, i.result
        FROM instances i
        JOIN instance_files if2 ON i.hash = if2.hash
        JOIN instance_tracks it ON i.hash = it.hash
        WHERE it.value = ?
    """
    params = [track]

    if result_filter:
        query += " AND i.result = ?"
        params.append(result_filter)

    query += " ORDER BY if2.value LIMIT ?"
    params.append(limit)

    cursor.execute(query, params)
    rows = cursor.fetchall()
    conn.close()
    return rows


def main():
    parser = argparse.ArgumentParser(description="Download competition instances from GBD")
    parser.add_argument("--track", help="Competition track (e.g., random_2002, main_2024)")
    parser.add_argument("--result", help="Filter by result (sat, unsat, unknown)")
    parser.add_argument("--limit", type=int, default=20, help="Max instances to download")
    parser.add_argument("--hashes", nargs="+", help="Specific hashes to download")
    parser.add_argument("--list-tracks", action="store_true", help="List available tracks")
    parser.add_argument("--output-dir", type=Path, help="Output directory (default: benchmarks/competition/<track>)")

    args = parser.parse_args()

    if args.list_tracks:
        conn = sqlite3.connect(str(BENCHMARKS_DB))
        cursor = conn.cursor()
        cursor.execute("""
            SELECT it.value, COUNT(*) as cnt,
                   SUM(CASE WHEN i.result = 'sat' THEN 1 ELSE 0 END) as sat_cnt
            FROM instance_tracks it
            JOIN instances i ON it.hash = i.hash
            GROUP BY it.value
            ORDER BY cnt DESC
        """)
        print(f"{'Track':<25} {'Total':<8} {'SAT':<8}")
        print("-" * 41)
        for track, cnt, sat in cursor.fetchall():
            print(f"{track:<25} {cnt:<8} {sat:<8}")
        conn.close()
        return

    if args.hashes:
        conn = sqlite3.connect(str(BENCHMARKS_DB))
        cursor = conn.cursor()
        instances = []
        for h in args.hashes:
            cursor.execute("""
                SELECT i.hash, if2.value, i.family, i.result
                FROM instances i
                JOIN instance_files if2 ON i.hash = if2.hash
                WHERE i.hash = ?
            """, (h,))
            row = cursor.fetchone()
            if row:
                instances.append(row)
        conn.close()
    elif args.track:
        instances = get_instances_by_track(args.track, args.result, args.limit)
    else:
        parser.print_help()
        return

    if not instances:
        print("No instances found.")
        return

    output_dir = args.output_dir or COMPETITION_DIR / (args.track or "manual")

    print(f"Downloading {len(instances)} instances to {output_dir}")
    downloaded = 0
    cached = 0
    failed = 0

    for hash_id, filename, family, result in instances:
        path, status = download_instance(hash_id, filename, output_dir)
        if status == "downloaded":
            downloaded += 1
            print(f"  [DL] {filename} ({family}, {result})")
        elif status == "cached":
            cached += 1
            print(f"  [OK] {filename} (cached)")
        else:
            failed += 1
            print(f"  [!!] {filename}: {status}")

    print(f"\nDone: {downloaded} downloaded, {cached} cached, {failed} failed")
    print(f"Location: {output_dir}")


if __name__ == "__main__":
    main()
