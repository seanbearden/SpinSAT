#!/usr/bin/env python3
"""
Compare benchmark results across runs and solvers.

Usage:
    python3 scripts/compare_results.py                    # compare all results
    python3 scripts/compare_results.py --latest 3         # compare last 3 runs
    python3 scripts/compare_results.py --tag v0.1 v0.2    # compare specific tags
    python3 scripts/compare_results.py --by-size           # breakdown by instance size
"""

import argparse
import json
import os
from pathlib import Path
from collections import defaultdict

RESULTS_DIR = Path(__file__).parent.parent / "results"


def load_results(filter_tags=None, latest_n=None):
    """Load result files, optionally filtered."""
    results = []
    for fpath in sorted(RESULTS_DIR.glob("*.json")):
        with open(fpath) as f:
            data = json.load(f)
            data["_file"] = fpath.name
            results.append(data)

    if filter_tags:
        results = [r for r in results if r.get("tag") in filter_tags]

    if latest_n:
        results = results[-latest_n:]

    return results


def compare_runs(results_list):
    """Print comparison table across runs."""
    print("=" * 80)
    print("BENCHMARK COMPARISON")
    print("=" * 80)

    # Header
    header = f"{'Run ID':<20} {'Tag':<12} {'Solver':<10} {'Solved':<8} {'T/O':<5} {'Median':<8} {'Max':<8} {'PAR-2':<10}"
    print(header)
    print("-" * 80)

    for result in results_list:
        for solver_name in result.get("solvers", []):
            solved = 0
            timeouts = 0
            total = 0
            times = []
            timeout_s = result.get("timeout_s", 60)

            for inst in result.get("instances", []):
                r = inst.get(solver_name, {})
                if "error" in r:
                    continue
                total += 1
                if r.get("status") == "SATISFIABLE":
                    solved += 1
                    times.append(r["time_s"])
                elif r.get("status") == "TIMEOUT":
                    timeouts += 1

            median_t = sorted(times)[len(times) // 2] if times else 0
            max_t = max(times) if times else 0
            total_time = sum(times)
            par2 = total_time + timeouts * 2 * timeout_s

            tag = result.get("tag", "")[:11]
            run_id = result.get("run_id", "")[:19]

            print(f"{run_id:<20} {tag:<12} {solver_name:<10} {solved}/{total:<5} {timeouts:<5} {median_t:<8.3f} {max_t:<8.3f} {par2:<10.2f}")

    print()


def breakdown_by_size(results_list):
    """Show performance breakdown by instance size."""
    print("=" * 80)
    print("PERFORMANCE BY INSTANCE SIZE")
    print("=" * 80)

    for result in results_list:
        tag = result.get("tag", result.get("run_id", ""))
        print(f"\n--- {tag} ---")

        for solver_name in result.get("solvers", []):
            print(f"  Solver: {solver_name}")

            # Group by variable count buckets
            buckets = defaultdict(lambda: {"solved": 0, "total": 0, "times": [], "timeouts": 0})

            for inst in result.get("instances", []):
                r = inst.get(solver_name, {})
                if "error" in r:
                    continue
                n_vars = r.get("num_vars", 0)
                # Bucket by order of magnitude
                if n_vars <= 50:
                    bucket = "≤50"
                elif n_vars <= 100:
                    bucket = "51-100"
                elif n_vars <= 250:
                    bucket = "101-250"
                elif n_vars <= 500:
                    bucket = "251-500"
                elif n_vars <= 1000:
                    bucket = "501-1000"
                else:
                    bucket = ">1000"

                b = buckets[bucket]
                b["total"] += 1
                if r.get("status") == "SATISFIABLE":
                    b["solved"] += 1
                    b["times"].append(r["time_s"])
                elif r.get("status") == "TIMEOUT":
                    b["timeouts"] += 1

            print(f"    {'Size':<12} {'Solved':<10} {'T/O':<5} {'Median':<10} {'Max':<10}")
            for bucket in ["≤50", "51-100", "101-250", "251-500", "501-1000", ">1000"]:
                if bucket not in buckets:
                    continue
                b = buckets[bucket]
                median_t = sorted(b["times"])[len(b["times"]) // 2] if b["times"] else 0
                max_t = max(b["times"]) if b["times"] else 0
                print(f"    {bucket:<12} {b['solved']}/{b['total']:<7} {b['timeouts']:<5} {median_t:<10.3f} {max_t:<10.3f}")


def show_progress(results_list, solver="spinsat"):
    """Show how a solver's performance has changed over time."""
    print("=" * 80)
    print(f"PROGRESS OVER TIME: {solver}")
    print("=" * 80)

    runs = []
    for result in results_list:
        if solver not in result.get("solvers", []):
            continue

        solved = 0
        total = 0
        times = []
        timeouts = 0

        for inst in result.get("instances", []):
            r = inst.get(solver, {})
            if "error" in r:
                continue
            total += 1
            if r.get("status") == "SATISFIABLE":
                solved += 1
                times.append(r["time_s"])
            elif r.get("status") == "TIMEOUT":
                timeouts += 1

        if total > 0:
            runs.append({
                "tag": result.get("tag", ""),
                "run_id": result.get("run_id", ""),
                "solved": solved,
                "total": total,
                "timeouts": timeouts,
                "median": sorted(times)[len(times) // 2] if times else 0,
                "mean": sum(times) / len(times) if times else 0,
                "max": max(times) if times else 0,
                "par2": sum(times) + timeouts * 2 * result.get("timeout_s", 60),
            })

    print(f"{'Run':<20} {'Tag':<12} {'Solved':<8} {'T/O':<5} {'Median':<8} {'Mean':<8} {'PAR-2':<10}")
    print("-" * 70)
    for r in runs:
        print(f"{r['run_id']:<20} {r['tag']:<12} {r['solved']}/{r['total']:<5} {r['timeouts']:<5} {r['median']:<8.3f} {r['mean']:<8.3f} {r['par2']:<10.2f}")

    if len(runs) >= 2:
        first = runs[0]
        last = runs[-1]
        par2_change = last["par2"] - first["par2"]
        solved_change = last["solved"] - first["solved"]
        print(f"\nProgress: solved {solved_change:+d}, PAR-2 {par2_change:+.2f}")


def main():
    parser = argparse.ArgumentParser(description="Compare SpinSAT benchmark results")
    parser.add_argument("--latest", type=int, help="Show only latest N runs")
    parser.add_argument("--tag", nargs="+", help="Filter by tag(s)")
    parser.add_argument("--by-size", action="store_true", help="Breakdown by instance size")
    parser.add_argument("--progress", action="store_true", help="Show progress over time")
    parser.add_argument("--solver", default="spinsat", help="Solver for progress view")

    args = parser.parse_args()

    results = load_results(filter_tags=args.tag, latest_n=args.latest)

    if not results:
        print("No results found in results/ directory.")
        print("Run: python3 scripts/benchmark_suite.py --suite small --solver spinsat")
        return

    compare_runs(results)

    if args.by_size:
        breakdown_by_size(results)

    if args.progress:
        show_progress(results, args.solver)


if __name__ == "__main__":
    main()
