#!/usr/bin/env python3
"""
A/B comparison of restart modes on the same instances.
Runs spinsat with cold, warm, and cycling modes, same seed.

Usage:
    python3 scripts/compare_restart_modes.py benchmarks/competition/anni_random/*.cnf
    python3 scripts/compare_restart_modes.py --timeout 120 benchmarks/competition/anni_random/*.cnf
"""

import subprocess
import sys
import time
import os
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent
SOLVER = str(PROJECT_ROOT / "target" / "release" / "spinsat")

MODES = ["cold", "warm", "cycling"]


def run_instance(cnf_path, mode, timeout=120, seed=42):
    """Run spinsat with a specific restart mode."""
    cmd = [SOLVER, "-r", mode, "-s", str(seed), "-t", str(timeout), cnf_path]
    start = time.time()
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout + 10)
        elapsed = time.time() - start
        status = "SAT" if "SATISFIABLE" in result.stdout else "T/O"

        # Extract restart count from stderr
        restarts = 0
        for line in result.stderr.splitlines():
            if "Solved after" in line:
                parts = line.split("after ")[1].split(" restart")
                restarts = int(parts[0])
            elif "Restart " in line:
                parts = line.split("Restart ")[1].split(" ")
                restarts = max(restarts, int(parts[0]))

        return elapsed, status, restarts
    except subprocess.TimeoutExpired:
        return timeout, "T/O", -1


def main():
    timeout = 120
    instances = []

    args = sys.argv[1:]
    i = 0
    while i < len(args):
        if args[i] == "--timeout":
            i += 1
            timeout = int(args[i])
        else:
            instances.append(args[i])
        i += 1

    if not instances:
        print("Usage: compare_restart_modes.py [--timeout N] <instance1.cnf> [instance2.cnf ...]")
        return

    # Header
    print(f"{'Instance':<45}", end="")
    for mode in MODES:
        print(f" {'Time':>7} {'St':>3} {'R':>3}", end="")
    print(f"  {'Winner':<8} {'Speedup':>7}")
    print("-" * (45 + len(MODES) * 14 + 16))

    totals = {m: {"time": 0.0, "solved": 0, "timeouts": 0} for m in MODES}

    for cnf in sorted(instances):
        name = os.path.basename(cnf)[:44]
        print(f"{name:<45}", end="", flush=True)

        results = {}
        for mode in MODES:
            t, status, restarts = run_instance(cnf, mode, timeout)
            results[mode] = (t, status, restarts)
            totals[mode]["time"] += t
            if status == "SAT":
                totals[mode]["solved"] += 1
            else:
                totals[mode]["timeouts"] += 1

            st = "S" if status == "SAT" else "T"
            r_str = str(restarts) if restarts >= 0 else "?"
            print(f" {t:>6.1f}s {st:>3} {r_str:>3}", end="", flush=True)

        # Determine winner
        sat_results = {m: r for m, r in results.items() if r[1] == "SAT"}
        if sat_results:
            winner = min(sat_results, key=lambda m: sat_results[m][0])
            best_time = sat_results[winner][0]
            # Speedup over cold
            cold_time = results["cold"][0]
            if results["cold"][1] == "SAT" and best_time > 0.001:
                speedup = cold_time / best_time
                print(f"  {winner:<8} {speedup:>6.2f}x")
            elif results["cold"][1] != "SAT":
                print(f"  {winner:<8}    NEW!")
            else:
                print(f"  {winner:<8}")
        else:
            print(f"  {'---':<8}")

    # Summary
    print()
    print("=" * (45 + len(MODES) * 14 + 16))
    print(f"{'SUMMARY':<45}", end="")
    for mode in MODES:
        t = totals[mode]
        print(f" {t['time']:>6.1f}s {t['solved']:>3} {t['timeouts']:>3}", end="")
    print()

    print(f"{'PAR-2':<45}", end="")
    for mode in MODES:
        t = totals[mode]
        par2 = t["time"] + t["timeouts"] * 2 * timeout
        # Only count time for solved instances
        solved_time = t["time"] - t["timeouts"] * timeout
        par2_correct = solved_time + t["timeouts"] * 2 * timeout
        print(f" {par2_correct:>7.1f}{'':>7}", end="")
    print()

    # Winner
    par2_scores = {}
    for mode in MODES:
        t = totals[mode]
        solved_time = t["time"] - t["timeouts"] * timeout
        par2_scores[mode] = solved_time + t["timeouts"] * 2 * timeout

    best_mode = min(par2_scores, key=lambda m: par2_scores[m])
    print(f"\nBest mode: {best_mode} (PAR-2: {par2_scores[best_mode]:.1f})")
    for mode in MODES:
        if mode != best_mode:
            ratio = par2_scores[mode] / par2_scores[best_mode]
            print(f"  vs {mode}: {ratio:.2f}x worse")


if __name__ == "__main__":
    main()
