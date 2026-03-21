#!/usr/bin/env python3
"""
Controlled performance comparison between solver versions.
Runs the same instances with the same seed, measures wall time.
Usage: python3 scripts/perf_compare.py <solver_a> <solver_b> [instances...]
"""
import subprocess
import sys
import time

def run_solver(solver, instance, seed=1, timeout=60, method="euler"):
    start = time.time()
    try:
        result = subprocess.run(
            [solver, "-m", method, "-s", str(seed), "-t", str(timeout), instance],
            capture_output=True, text=True, timeout=timeout + 5
        )
        elapsed = time.time() - start
        # Extract step count from stderr
        steps = "?"
        for line in result.stderr.splitlines():
            if "Solved" in line:
                # "c Solved after N restarts, step XXXX (t=...)"
                parts = line.split("step ")
                if len(parts) > 1:
                    steps = parts[1].split(" ")[0]
            elif "Restart" in line:
                steps = "restart"
        status = "SAT" if "SATISFIABLE" in result.stdout else "T/O"
        return elapsed, status, steps
    except subprocess.TimeoutExpired:
        return timeout, "T/O", "?"

def main():
    if len(sys.argv) < 4:
        print("Usage: perf_compare.py <solver_a> <solver_b> <instance1> [instance2 ...]")
        return

    solver_a = sys.argv[1]
    solver_b = sys.argv[2]
    instances = sys.argv[3:]

    print(f"{'Instance':<40} {'A time':>8} {'A steps':>10} {'B time':>8} {'B steps':>10} {'Speedup':>8}")
    print("-" * 86)

    total_a = 0
    total_b = 0
    count = 0

    for inst in instances:
        name = inst.split("/")[-1]
        ta, sa, stepa = run_solver(solver_a, inst)
        tb, sb, stepb = run_solver(solver_b, inst)

        if sa == "SAT" and sb == "SAT":
            speedup = tb / ta if ta > 0.001 else 0
            total_a += ta
            total_b += tb
            count += 1
            print(f"{name:<40} {ta:>7.3f}s {stepa:>10} {tb:>7.3f}s {stepb:>10} {speedup:>7.2f}x")
        else:
            print(f"{name:<40} {ta:>7.3f}s {sa:<10} {tb:>7.3f}s {sb:<10}")

    if count > 0:
        print("-" * 86)
        print(f"{'TOTAL':<40} {total_a:>7.3f}s {'':>10} {total_b:>7.3f}s {'':>10} {total_b/total_a:>7.2f}x")

if __name__ == "__main__":
    main()
