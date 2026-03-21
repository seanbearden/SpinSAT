#!/usr/bin/env python3
"""
SpinSAT Benchmark Suite

Runs solvers against a curated set of CNF instances, records results as JSON.
Supports multiple solvers for baseline comparison (e.g., Kissat vs SpinSAT).

Usage:
    python3 scripts/benchmark_suite.py [--solver spinsat] [--solver kissat] [--timeout 60] [--tag v0.1]
    python3 scripts/benchmark_suite.py --suite small --solver spinsat --timeout 30
    python3 scripts/benchmark_suite.py --instances benchmarks/competition/UF250.1065.100/*.cnf --solver spinsat
"""

import argparse
import json
import os
import subprocess
import sys
import time
import glob
from datetime import datetime
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent
RESULTS_DIR = PROJECT_ROOT / "results"
BENCHMARKS_DIR = PROJECT_ROOT / "benchmarks"
CHECKER = PROJECT_ROOT / "scripts" / "check_sat"

# Solver configurations
SOLVERS = {
    "spinsat": {
        "cmd": str(PROJECT_ROOT / "target" / "release" / "spinsat"),
        "parse_output": "competition",  # s SATISFIABLE / v ... format
    },
    "kissat": {
        "cmd": "kissat",
        "args": ["-q"],  # quiet mode
        "parse_output": "competition",
    },
}

# Predefined benchmark suites
SUITES = {
    "tiny": {
        "description": "Quick smoke test (20-50 vars)",
        "patterns": [
            "tests/test1.cnf",
        ],
        "generated": [
            {"n": 20, "ratio": 4.3, "count": 5},
            {"n": 50, "ratio": 4.3, "count": 5},
        ],
    },
    "small": {
        "description": "Small instances (100-250 vars)",
        "generated": [
            {"n": 100, "ratio": 4.3, "count": 10},
            {"n": 150, "ratio": 4.3, "count": 10},
            {"n": 200, "ratio": 4.3, "count": 10},
            {"n": 250, "ratio": 4.3, "count": 10},
        ],
    },
    "medium": {
        "description": "Medium instances (250-500 vars)",
        "generated": [
            {"n": 250, "ratio": 4.3, "count": 20},
            {"n": 350, "ratio": 4.3, "count": 10},
            {"n": 500, "ratio": 4.3, "count": 10},
        ],
    },
    "large": {
        "description": "Large instances (500-2000 vars)",
        "generated": [
            {"n": 500, "ratio": 4.3, "count": 10},
            {"n": 750, "ratio": 4.3, "count": 5},
            {"n": 1000, "ratio": 4.3, "count": 5},
            {"n": 1500, "ratio": 4.3, "count": 3},
            {"n": 2000, "ratio": 4.3, "count": 3},
        ],
    },
    "uf250": {
        "description": "SATLIB UF250 benchmark (100 instances, ratio 4.26)",
        "patterns": [
            "benchmarks/competition/UF250.1065.100/*.cnf",
        ],
    },
}


def generate_instance(n_vars, ratio, seed, output_dir):
    """Generate a planted random 3-SAT instance."""
    output_dir = Path(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    fname = f"planted_n{n_vars}_r{ratio}_s{seed}.cnf"
    fpath = output_dir / fname
    if fpath.exists():
        return str(fpath)

    gen_script = PROJECT_ROOT / "scripts" / "gen_random_sat.py"
    result = subprocess.run(
        [sys.executable, str(gen_script), str(n_vars), str(ratio), str(seed)],
        capture_output=True, text=True
    )
    fpath.write_text(result.stdout)
    return str(fpath)


def get_instance_info(cnf_path):
    """Extract num_vars and num_clauses from CNF header."""
    with open(cnf_path) as f:
        for line in f:
            line = line.strip()
            if line.startswith("p cnf"):
                parts = line.split()
                return int(parts[2]), int(parts[3])
    return 0, 0


def parse_solver_output(stdout, stderr=""):
    """Parse competition-format solver output."""
    status = "UNKNOWN"
    assignment = []
    for line in stdout.splitlines():
        line = line.strip()
        if line.startswith("s "):
            status = line[2:]
        elif line.startswith("v "):
            for tok in line[2:].split():
                val = int(tok)
                if val != 0:
                    assignment.append(val)
    return status, assignment


def verify_solution(cnf_path, solver_output_path):
    """Verify solution using the C checker."""
    if not CHECKER.exists():
        return "CHECKER_MISSING"
    result = subprocess.run(
        [str(CHECKER), cnf_path, solver_output_path],
        capture_output=True, text=True, timeout=30
    )
    if "OK" in result.stdout:
        return "VERIFIED"
    elif "FAIL" in result.stderr or "FAIL" in result.stdout:
        return "WRONG"
    return "UNKNOWN"


def run_solver(solver_name, cnf_path, timeout):
    """Run a solver on a CNF instance and return results."""
    config = SOLVERS.get(solver_name)
    if not config:
        return {"error": f"Unknown solver: {solver_name}"}

    cmd = [config["cmd"]]
    cmd.extend(config.get("args", []))
    cmd.append(cnf_path)

    num_vars, num_clauses = get_instance_info(cnf_path)
    ratio = num_clauses / num_vars if num_vars > 0 else 0

    start = time.time()
    try:
        result = subprocess.run(
            cmd, capture_output=True, text=True,
            timeout=timeout
        )
        elapsed = time.time() - start
        status, assignment = parse_solver_output(result.stdout, result.stderr)

        # Verify if SAT
        verified = "N/A"
        if status == "SATISFIABLE" and CHECKER.exists():
            import tempfile
            with tempfile.NamedTemporaryFile(mode='w', suffix='.txt', delete=False) as f:
                f.write(result.stdout)
                tmppath = f.name
            verified = verify_solution(cnf_path, tmppath)
            os.unlink(tmppath)

        return {
            "status": status,
            "time_s": round(elapsed, 4),
            "verified": verified,
            "num_vars": num_vars,
            "num_clauses": num_clauses,
            "ratio": round(ratio, 3),
        }

    except subprocess.TimeoutExpired:
        elapsed = time.time() - start
        return {
            "status": "TIMEOUT",
            "time_s": round(elapsed, 4),
            "verified": "N/A",
            "num_vars": num_vars,
            "num_clauses": num_clauses,
            "ratio": round(ratio, 3),
        }
    except FileNotFoundError:
        return {"error": f"Solver not found: {config['cmd']}"}


def collect_instances(suite_name=None, instance_patterns=None):
    """Collect CNF instances from a suite definition or glob patterns."""
    instances = []

    if instance_patterns:
        for pattern in instance_patterns:
            instances.extend(sorted(glob.glob(pattern)))
        return instances

    if suite_name and suite_name in SUITES:
        suite = SUITES[suite_name]

        # File patterns
        for pattern in suite.get("patterns", []):
            full_pattern = str(PROJECT_ROOT / pattern)
            instances.extend(sorted(glob.glob(full_pattern)))

        # Generated instances
        for gen in suite.get("generated", []):
            for seed in range(1, gen["count"] + 1):
                path = generate_instance(
                    gen["n"], gen["ratio"], seed,
                    BENCHMARKS_DIR / "generated" / f"n{gen['n']}_r{gen['ratio']}"
                )
                instances.append(path)

    return instances


def run_benchmark(solvers, instances, timeout, tag=""):
    """Run benchmark suite and return results."""
    run_id = datetime.now().strftime("%Y%m%d_%H%M%S")
    results = {
        "run_id": run_id,
        "tag": tag,
        "timestamp": datetime.now().isoformat(),
        "timeout_s": timeout,
        "solvers": solvers,
        "instances": [],
    }

    total = len(instances) * len(solvers)
    current = 0

    for cnf_path in instances:
        instance_name = os.path.basename(cnf_path)
        instance_results = {"instance": instance_name, "path": cnf_path}

        for solver_name in solvers:
            current += 1
            print(f"[{current}/{total}] {solver_name}: {instance_name}...", end=" ", flush=True)

            result = run_solver(solver_name, cnf_path, timeout)

            if "error" in result:
                print(f"ERROR: {result['error']}")
                instance_results[solver_name] = result
                continue

            status_char = {
                "SATISFIABLE": "SAT",
                "UNSATISFIABLE": "UNSAT",
                "TIMEOUT": "T/O",
                "UNKNOWN": "UNK",
            }.get(result["status"], "???")

            verified_char = ""
            if result.get("verified") == "VERIFIED":
                verified_char = " [OK]"
            elif result.get("verified") == "WRONG":
                verified_char = " [WRONG!]"

            print(f"{status_char} {result['time_s']:.2f}s{verified_char}")
            instance_results[solver_name] = result

        results["instances"].append(instance_results)

    return results


def save_results(results, suite_name="custom"):
    """Save results to JSON file."""
    RESULTS_DIR.mkdir(parents=True, exist_ok=True)
    tag = results.get("tag", "")
    tag_str = f"_{tag}" if tag else ""
    fname = f"{results['run_id']}_{suite_name}{tag_str}.json"
    fpath = RESULTS_DIR / fname
    with open(fpath, 'w') as f:
        json.dump(results, f, indent=2)
    return fpath


def print_summary(results):
    """Print summary table."""
    print("\n" + "=" * 70)
    print("BENCHMARK SUMMARY")
    print("=" * 70)
    print(f"Run ID: {results['run_id']}")
    print(f"Tag: {results.get('tag', 'N/A')}")
    print(f"Timeout: {results['timeout_s']}s")
    print()

    for solver_name in results["solvers"]:
        solved = 0
        timeouts = 0
        wrong = 0
        total_time = 0
        times = []
        total = 0

        for inst in results["instances"]:
            r = inst.get(solver_name, {})
            if "error" in r:
                continue
            total += 1
            if r.get("status") == "SATISFIABLE":
                solved += 1
                total_time += r["time_s"]
                times.append(r["time_s"])
                if r.get("verified") == "WRONG":
                    wrong += 1
            elif r.get("status") == "TIMEOUT":
                timeouts += 1

        avg_time = total_time / solved if solved > 0 else 0
        max_time = max(times) if times else 0
        median_time = sorted(times)[len(times) // 2] if times else 0

        print(f"--- {solver_name} ---")
        print(f"  Solved:   {solved}/{total}")
        print(f"  Timeouts: {timeouts}")
        print(f"  Wrong:    {wrong}")
        if solved > 0:
            print(f"  Avg time: {avg_time:.3f}s")
            print(f"  Median:   {median_time:.3f}s")
            print(f"  Max time: {max_time:.3f}s")
            print(f"  Total:    {total_time:.2f}s")

            # PAR-2 score
            par2 = total_time + timeouts * 2 * results["timeout_s"]
            print(f"  PAR-2:    {par2:.2f}")
        print()


def main():
    parser = argparse.ArgumentParser(description="SpinSAT Benchmark Suite")
    parser.add_argument("--suite", choices=list(SUITES.keys()),
                        help="Predefined benchmark suite to run")
    parser.add_argument("--instances", nargs="+",
                        help="Specific CNF files or glob patterns")
    parser.add_argument("--solver", action="append", dest="solvers",
                        help="Solver(s) to benchmark (can repeat)")
    parser.add_argument("--timeout", type=int, default=60,
                        help="Timeout per instance in seconds (default: 60)")
    parser.add_argument("--tag", default="",
                        help="Tag for this run (e.g., 'v0.1', 'after-rk4')")
    parser.add_argument("--list-suites", action="store_true",
                        help="List available benchmark suites")

    args = parser.parse_args()

    if args.list_suites:
        print("Available benchmark suites:")
        for name, suite in SUITES.items():
            print(f"  {name:10s} — {suite['description']}")
        return

    solvers = args.solvers or ["spinsat"]
    suite_name = args.suite or "custom"

    # Verify solvers exist
    for s in solvers:
        if s not in SOLVERS:
            print(f"Unknown solver: {s}")
            print(f"Available: {', '.join(SOLVERS.keys())}")
            return

    # Collect instances
    instances = collect_instances(args.suite, args.instances)
    if not instances:
        print("No instances found. Use --suite or --instances.")
        return

    print(f"Suite: {suite_name}")
    print(f"Instances: {len(instances)}")
    print(f"Solvers: {', '.join(solvers)}")
    print(f"Timeout: {args.timeout}s")
    print(f"Tag: {args.tag or '(none)'}")
    print()

    # Run benchmark
    results = run_benchmark(solvers, instances, args.timeout, args.tag)

    # Save and summarize
    fpath = save_results(results, suite_name)
    print_summary(results)
    print(f"Results saved to: {fpath}")


if __name__ == "__main__":
    main()
