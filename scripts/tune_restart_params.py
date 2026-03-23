#!/usr/bin/env python3
"""
Tune warm restart parameters: xl_decay and noise_scale.
Tests combinations on hard instances and reports PAR-2.

Usage:
    python3 scripts/tune_restart_params.py benchmarks/competition/random_2007/unif-k3-r4.26-v{360,400}*.cnf
"""

import subprocess
import sys
import time
import os
from pathlib import Path
from itertools import product

PROJECT_ROOT = Path(__file__).parent.parent
SOLVER = str(PROJECT_ROOT / "target" / "release" / "spinsat")

DECAY_VALUES = [0.0, 0.1, 0.3, 0.5, 0.7]
NOISE_VALUES = [0.05, 0.1, 0.2, 0.3]


def run_instance(cnf_path, xl_decay, noise, timeout=120, seed=42):
    cmd = [SOLVER, "-r", "cycling",
           "--xl-decay", str(xl_decay),
           "--restart-noise", str(noise),
           "-s", str(seed), "-t", str(timeout), cnf_path]
    start = time.time()
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout + 10)
        elapsed = time.time() - start
        status = "SAT" if "SATISFIABLE" in result.stdout else "T/O"
        return elapsed, status
    except subprocess.TimeoutExpired:
        return timeout, "T/O"


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
        print("Usage: tune_restart_params.py [--timeout N] <instances...>")
        return

    print(f"Tuning xl_decay x noise on {len(instances)} instances (timeout={timeout}s)")
    print(f"Decay values: {DECAY_VALUES}")
    print(f"Noise values: {NOISE_VALUES}")
    print(f"Total configs: {len(DECAY_VALUES) * len(NOISE_VALUES)}")
    print()

    results = {}

    for decay, noise in product(DECAY_VALUES, NOISE_VALUES):
        key = (decay, noise)
        solved = 0
        timeouts = 0
        total_time = 0.0

        for cnf in sorted(instances):
            t, status = run_instance(cnf, decay, noise, timeout)
            if status == "SAT":
                solved += 1
                total_time += t
            else:
                timeouts += 1
                total_time += timeout

        par2 = (total_time - timeouts * timeout) + timeouts * 2 * timeout
        results[key] = {"solved": solved, "timeouts": timeouts, "par2": par2, "total_time": total_time}

        print(f"  decay={decay:.1f} noise={noise:.2f}: {solved}/{len(instances)} solved, PAR-2={par2:.1f}")

    # Summary table
    print()
    print("=" * 70)
    print("PARAMETER TUNING RESULTS")
    print("=" * 70)
    print(f"{'Decay':>6} {'Noise':>6} {'Solved':>7} {'T/O':>4} {'PAR-2':>10}")
    print("-" * 40)

    sorted_results = sorted(results.items(), key=lambda x: x[1]["par2"])
    for (decay, noise), r in sorted_results:
        marker = " ***" if (decay, noise) == sorted_results[0][0] else ""
        print(f"{decay:>6.1f} {noise:>6.2f} {r['solved']:>4}/{len(instances):<2} {r['timeouts']:>4} {r['par2']:>10.1f}{marker}")

    best = sorted_results[0]
    print(f"\nBest: decay={best[0][0]}, noise={best[0][1]} (PAR-2: {best[1]['par2']:.1f})")


if __name__ == "__main__":
    main()
