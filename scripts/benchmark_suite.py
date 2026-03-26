#!/usr/bin/env python3
"""
SpinSAT Benchmark Suite

Runs solvers against a curated set of CNF instances, records results as JSON.
Supports multiple solvers for baseline comparison (e.g., Kissat vs SpinSAT).

Usage:
    python3 scripts/benchmark_suite.py [--solver spinsat] [--solver kissat] [--timeout 60] [--tag v0.1]
    python3 scripts/benchmark_suite.py --suite small --solver spinsat --timeout 30
    python3 scripts/benchmark_suite.py --suite large --record --tag v0.4.0
    python3 scripts/benchmark_suite.py --instances benchmarks/competition/UF250.1065.100/*.cnf --solver spinsat
"""

import argparse
import hashlib
import json
import os
import platform
import re
import resource
import sqlite3
import subprocess
import sys
import time
import glob
import uuid
from datetime import datetime, timezone
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent
RESULTS_DIR = PROJECT_ROOT / "results"
BENCHMARKS_DIR = PROJECT_ROOT / "benchmarks"
BENCHMARKS_DB = PROJECT_ROOT / "benchmarks.db"
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


# ---------------------------------------------------------------------------
# Auto-detection helpers
# ---------------------------------------------------------------------------

def detect_solver_version(solver_cmd):
    """Auto-detect solver version from --version output, falling back to Cargo.toml."""
    try:
        result = subprocess.run(
            [solver_cmd, "--version"],
            capture_output=True, text=True, timeout=5
        )
        # Parse "spinsat 0.4.0" format
        output = result.stdout.strip()
        parts = output.split()
        if len(parts) >= 2:
            return parts[1]
        if output:
            return output
    except (subprocess.TimeoutExpired, FileNotFoundError, OSError):
        pass

    # Fallback: read version from Cargo.toml (works when binary is cross-compiled)
    cargo_toml = PROJECT_ROOT / "Cargo.toml"
    if cargo_toml.exists():
        for line in cargo_toml.read_text().splitlines():
            if line.strip().startswith("version") and "=" in line:
                ver = line.split("=", 1)[1].strip().strip('"').strip("'")
                if ver:
                    return ver
    return "unknown"


def detect_git_info():
    """Auto-detect git commit and dirty state."""
    try:
        commit = subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"],
            capture_output=True, text=True, timeout=5,
            cwd=str(PROJECT_ROOT)
        ).stdout.strip()

        dirty_result = subprocess.run(
            ["git", "diff", "--quiet"],
            capture_output=True, timeout=5,
            cwd=str(PROJECT_ROOT)
        )
        dirty = dirty_result.returncode != 0

        return commit, dirty
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return "unknown", False


def detect_rust_version():
    """Auto-detect Rust compiler version."""
    try:
        result = subprocess.run(
            ["rustc", "--version"],
            capture_output=True, text=True, timeout=5
        )
        return result.stdout.strip()
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return "unknown"


def detect_hardware():
    """Auto-detect hardware description."""
    machine = platform.machine()
    processor = platform.processor() or machine
    system = platform.system()
    return f"{system} {processor} ({machine})"


def compute_instance_hash(cnf_path):
    """Compute SHA-256 hash of a CNF file for instance identification."""
    h = hashlib.sha256()
    with open(cnf_path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            h.update(chunk)
    return h.hexdigest()


def parse_spinsat_stderr(stderr):
    """Parse SpinSAT stderr for parameters and solve metadata."""
    info = {
        "restarts": 0,
        "method_used": None,
        "zeta": None,
        "strategy": None,
        "seed": None,
        "peak_xl_max": None,
        "final_dt": None,
        "restart_strategy": None,
        "preprocessing": None,
        "cdcl_handoffs": 0,
        "solved_by": None,  # "dmm", "cadical", "preprocessing"
    }

    preprocess_techniques = []

    for line in stderr.splitlines():
        # "c Parameters: strategy=Fixed(Euler), restart_mode=Cycling, zeta=1e-3, seed=1"
        # Also handles legacy format without restart_mode
        m = re.search(r"strategy=(\S+),\s*(?:restart_mode=(\S+),\s*)?zeta=([^,]+),\s*seed=(\d+)", line)
        if m:
            info["strategy"] = m.group(1)
            if m.group(2) and not info.get("restart_strategy"):
                info["restart_strategy"] = m.group(2)
            try:
                info["zeta"] = float(m.group(3))
            except ValueError:
                pass
            info["seed"] = int(m.group(4))

        # "c Solved after 3 restarts using Euler (elapsed: 1.2s)"
        m = re.search(r"Solved after (\d+) restarts using (\S+)", line)
        if m:
            info["restarts"] = int(m.group(1))
            info["method_used"] = m.group(2)
            if not info["solved_by"]:
                info["solved_by"] = "dmm"

        # "c Restart 5 Euler Cycling ..." or "c Restart 5 Euler Cold ..."
        m = re.search(r"Restart (\d+)\s+\S+\s+(\S+)", line)
        if m:
            info["restarts"] = max(info["restarts"], int(m.group(1)))
            info["restart_strategy"] = m.group(2)

        # "c peak_xl_max: 2.982051e1"
        m = re.search(r"peak_xl_max:\s*([^\s]+)", line)
        if m:
            try:
                info["peak_xl_max"] = float(m.group(1))
            except ValueError:
                pass

        # "c final_dt: 1.689207e-1"
        m = re.search(r"final_dt:\s*([^\s]+)", line)
        if m:
            try:
                info["final_dt"] = float(m.group(1))
            except ValueError:
                pass

        # "c   unit_prop=0, pure_lit=3, subsump=0, self_sub=3, bve=0, probe=0"
        m = re.search(r"unit_prop=(\d+).*pure_lit=(\d+).*subsump=(\d+).*self_sub=(\d+).*bve=(\d+).*probe=(\d+)", line)
        if m:
            technique_names = ["unit_prop", "pure_lit", "subsump", "self_sub", "bve", "probe"]
            for name, count_str in zip(technique_names, m.groups()):
                if int(count_str) > 0:
                    preprocess_techniques.append(name)

        # "c Solved by preprocessing alone!"
        if "Solved by preprocessing alone" in line:
            preprocess_techniques.append("solved_by_preprocess")
            info["solved_by"] = "preprocessing"

        # "c UNSAT signal XlStagnation fired (handoff #3, elapsed: 12.5s, ...)"
        m = re.search(r"UNSAT signal (\S+) fired \(handoff #(\d+)", line)
        if m:
            info["cdcl_handoffs"] = int(m.group(2))

        # "c CaDiCaL found SAT (handoff #2, elapsed: 5.3s)"
        if "CaDiCaL found SAT" in line:
            info["solved_by"] = "cadical"

        # "c CaDiCaL proved UNSAT (handoff #3, elapsed: 8.1s)"
        if "CaDiCaL proved UNSAT" in line:
            info["solved_by"] = "cadical"

        # "c Adaptive CDCL proved UNSAT"
        if "Adaptive CDCL proved UNSAT" in line:
            info["solved_by"] = "cadical"

        # "c CDCL proved UNSAT" (final fallback)
        if re.search(r"^c CDCL proved UNSAT", line):
            info["solved_by"] = "cadical"

        # "c Preprocessing: 50 vars → 47, 215 clauses → 210 ..."
        if "Preprocessing:" in line:
            info["preprocessing"] = "enabled"

    # Summarize preprocessing
    if info["preprocessing"] == "enabled":
        if preprocess_techniques:
            info["preprocessing"] = ",".join(preprocess_techniques)
        else:
            info["preprocessing"] = "none_applied"
    elif "no-preprocess" not in str(info.get("strategy", "")):
        # No preprocessing output seen — likely disabled
        info["preprocessing"] = "disabled"

    return info


# ---------------------------------------------------------------------------
# Core benchmark functions
# ---------------------------------------------------------------------------

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

    start_wall = time.time()
    start_rusage = resource.getrusage(resource.RUSAGE_CHILDREN)
    try:
        result = subprocess.run(
            cmd, capture_output=True, text=True,
            timeout=timeout
        )
        wall_clock_s = round(time.time() - start_wall, 6)
        end_rusage = resource.getrusage(resource.RUSAGE_CHILDREN)
        cpu_time_s = round(
            (end_rusage.ru_utime - start_rusage.ru_utime)
            + (end_rusage.ru_stime - start_rusage.ru_stime), 6
        )

        status, assignment = parse_solver_output(result.stdout, result.stderr)

        # Parse SpinSAT-specific stderr
        stderr_info = {}
        if solver_name == "spinsat":
            stderr_info = parse_spinsat_stderr(result.stderr)

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
            "time_s": round(wall_clock_s, 4),
            "wall_clock_s": wall_clock_s,
            "cpu_time_s": cpu_time_s,
            "verified": verified,
            "num_vars": num_vars,
            "num_clauses": num_clauses,
            "ratio": round(ratio, 3),
            **stderr_info,
        }

    except subprocess.TimeoutExpired:
        wall_clock_s = round(time.time() - start_wall, 6)
        end_rusage = resource.getrusage(resource.RUSAGE_CHILDREN)
        cpu_time_s = round(
            (end_rusage.ru_utime - start_rusage.ru_utime)
            + (end_rusage.ru_stime - start_rusage.ru_stime), 6
        )
        return {
            "status": "TIMEOUT",
            "time_s": round(wall_clock_s, 4),
            "wall_clock_s": wall_clock_s,
            "cpu_time_s": cpu_time_s,
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


# ---------------------------------------------------------------------------
# Storage: JSON and SQLite
# ---------------------------------------------------------------------------

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


def update_competition_best(conn):
    """Update competition_best table for all benchmarked instances."""
    cursor = conn.cursor()
    cursor.execute("""
        INSERT OR REPLACE INTO competition_best
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
    updated = cursor.rowcount
    if updated > 0:
        print(f"  Updated {updated} rows in competition_best")


def record_to_db(results, solver_name, run_metadata):
    """Record benchmark results to benchmarks.db for the given solver."""
    if not BENCHMARKS_DB.exists():
        print(f"\nWarning: {BENCHMARKS_DB} not found.")
        print("Run: python3 scripts/init_benchmarks_db.py")
        return

    conn = sqlite3.connect(str(BENCHMARKS_DB))
    cursor = conn.cursor()

    run_id = run_metadata["run_id"]

    # Build legacy tag from structured fields if not provided
    tag = results.get("tag", "")
    if not tag and run_metadata.get("tag_instance_set"):
        parts = [run_metadata["solver_version"]]
        if run_metadata.get("tag_instance_set"):
            parts.append(run_metadata["tag_instance_set"])
        if run_metadata.get("tag_config") and run_metadata["tag_config"] != "default":
            parts.append(run_metadata["tag_config"])
        tag = "-".join(parts)

    # Insert run metadata
    cursor.execute("""
        INSERT OR REPLACE INTO runs
        (run_id, solver_version, git_commit, git_dirty, integration_method,
         strategy, timestamp, timeout_s, hardware, rust_version, tag, notes,
         restart_strategy, preprocessing, cli_command,
         tag_purpose, tag_instance_set, tag_config)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    """, (
        run_id,
        run_metadata["solver_version"],
        run_metadata["git_commit"],
        run_metadata["git_dirty"],
        run_metadata.get("integration_method"),
        run_metadata.get("strategy"),
        run_metadata["timestamp"],
        results["timeout_s"],
        run_metadata["hardware"],
        run_metadata.get("rust_version"),
        tag,
        run_metadata.get("notes"),
        run_metadata.get("restart_strategy"),
        run_metadata.get("preprocessing"),
        run_metadata.get("cli_command"),
        run_metadata.get("tag_purpose"),
        run_metadata.get("tag_instance_set"),
        run_metadata.get("tag_config"),
    ))

    # Insert per-instance results
    recorded = 0
    for inst in results["instances"]:
        r = inst.get(solver_name, {})
        if "error" in r:
            continue

        # Compute instance hash from file
        cnf_path = inst.get("path", "")
        if cnf_path and os.path.exists(cnf_path):
            instance_hash = compute_instance_hash(cnf_path)
        else:
            # Fallback: hash from instance name
            instance_hash = hashlib.sha256(
                inst.get("instance", "").encode()
            ).hexdigest()

        cursor.execute("""
            INSERT OR REPLACE INTO results
            (run_id, instance_hash, status, time_s, steps, restarts,
             verified, seed, zeta, alpha, beta, gamma, delta, epsilon,
             dt_min, dt_max, peak_xl_max, final_dt, wall_clock_s, cpu_time_s,
             num_vars, num_clauses, cdcl_handoffs, solved_by)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """, (
            run_id,
            instance_hash,
            r.get("status"),
            r.get("time_s"),
            r.get("steps"),
            r.get("restarts"),
            r.get("verified"),
            r.get("seed"),
            r.get("zeta"),
            None,  # alpha (not yet exposed in stderr)
            None,  # beta
            None,  # gamma
            None,  # delta
            None,  # epsilon
            None,  # dt_min
            None,  # dt_max
            r.get("peak_xl_max"),
            r.get("final_dt"),
            r.get("wall_clock_s"),
            r.get("cpu_time_s"),
            r.get("num_vars"),
            r.get("num_clauses"),
            r.get("cdcl_handoffs"),
            r.get("solved_by"),
        ))
        recorded += 1

    # Update competition_best for newly benchmarked instances
    update_competition_best(conn)

    conn.commit()
    conn.close()
    return recorded


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


def cloud_run(args, instances, suite_name):
    """Run benchmarks on a GCP instance under competition-faithful conditions."""
    from cloud_benchmark import CloudBenchmark

    cb = CloudBenchmark(
        zone=args.cloud_zone,
        machine_type=args.cloud_machine,
        spot=not args.cloud_on_demand,
        max_hours=args.cloud_max_hours,
        parallelism=args.cloud_parallelism,
        bucket=args.cloud_bucket,
        project=args.cloud_project,
    )

    if args.dry_run:
        cb.print_plan(instances, args.timeout)
        return

    run_id = datetime.now().strftime("%Y%m%d_%H%M%S")

    try:
        cb.create_instance()
        cb.upload_solver()
        cb.upload_worker_script()
        cb.upload_instances(instances)
        solver_args = args.solver_args.split() if args.solver_args else None
        remote_results = cb.run(timeout=args.timeout, tag=args.tag,
                                solver_args=solver_args)

        results = cb.download_results(
            remote_results,
            run_id=run_id,
            tag=args.tag,
            timeout_s=args.timeout,
        )
    except KeyboardInterrupt:
        print(f"\nInterrupted! Worker continues on VM: {cb.instance_name}")
        print(f"  Recover results: python3 scripts/benchmark_suite.py --cloud-recover {cb.instance_name} --cloud-zone {cb.zone}")
        print(f"  Delete manually: gcloud compute instances delete {cb.instance_name} --zone {cb.zone} --project {cb.project}")
        return
    except Exception as e:
        print(f"\nError: {e}")
        print(f"  VM kept alive for recovery: {cb.instance_name}")
        print(f"  Recover: python3 scripts/benchmark_suite.py --cloud-recover {cb.instance_name} --cloud-zone {cb.zone}")
        print(f"  Delete:  gcloud compute instances delete {cb.instance_name} --zone {cb.zone} --project {cb.project} --quiet")
        print(f"  NOTE: VM incurs cost until deleted! Auto-shutdown safety net: {cb.max_hours}h")
        return

    # Success — delete VM now that results are downloaded
    cb.delete_instance()

    # Save in standard format
    fpath = save_results(results, suite_name)
    print_summary(results)
    print(f"Results saved to: {fpath}")

    # Record to DB if --record
    if args.record:
        env = results.get("environment", {})
        solver_version = detect_solver_version(
            str(PROJECT_ROOT / "target" / "release" / "spinsat")
        )
        git_commit, git_dirty = detect_git_info()
        cli_command = " ".join(sys.argv)
        run_metadata = {
            "run_id": run_id,
            "solver_version": solver_version,
            "git_commit": git_commit,
            "git_dirty": git_dirty,
            "hardware": f"GCP {env.get('machine_type', 'unknown')} ({env.get('cpu_platform', 'unknown')})",
            "rust_version": detect_rust_version(),
            "timestamp": results.get("timestamp", datetime.now(timezone.utc).isoformat()),
            "notes": f"cloud run: {env.get('zone', '')}, spot={env.get('spot', '')}, parallelism={env.get('parallelism', '')}",
            "cli_command": cli_command,
            "tag_purpose": getattr(args, "purpose", None),
            "tag_instance_set": getattr(args, "instance_set", None),
            "tag_config": getattr(args, "config", None),
        }
        # Pick up strategy/method/restart/preprocessing from first result
        for inst in results.get("instances", []):
            r = inst.get("spinsat", {})
            if r.get("strategy"):
                run_metadata["strategy"] = r["strategy"]
                run_metadata["integration_method"] = r.get("method_used")
                if r.get("restart_strategy"):
                    run_metadata["restart_strategy"] = r["restart_strategy"]
                if r.get("preprocessing"):
                    run_metadata["preprocessing"] = r["preprocessing"]
                break

        recorded = record_to_db(results, "spinsat", run_metadata)
        if recorded:
            print(f"Recorded {recorded} results to {BENCHMARKS_DB}")
            print(f"  Run ID: {run_metadata['run_id']}")


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
    parser.add_argument("--record", action="store_true",
                        help="Record results to benchmarks.db (official run)")
    parser.add_argument("--notes", default="",
                        help="Notes for this recorded run")
    parser.add_argument("--force", action="store_true",
                        help="Record even with uncommitted changes (skip prompt)")
    parser.add_argument("--solver-args", default="",
                        help="Extra args passed to solver as a single string (e.g., --solver-args='-m euler')")

    # Structured tagging
    tag_group = parser.add_argument_group("structured tags (for --record)")
    tag_group.add_argument("--purpose",
                           choices=["paper-verification", "competition-benchmark",
                                    "regression-test", "development"],
                           help="Purpose of this benchmark run")
    tag_group.add_argument("--instance-set",
                           help="Instance set name (e.g., barthel, komb, anni2022)")
    tag_group.add_argument("--config",
                           help="Run config label (e.g., default, cycling, no-preprocess)")
    parser.add_argument("--list-suites", action="store_true",
                        help="List available benchmark suites")

    # Cloud benchmark flags
    cloud = parser.add_argument_group("cloud benchmarking (GCP)")
    cloud.add_argument("--cloud", action="store_true",
                       help="Run on GCP instead of locally")
    cloud.add_argument("--cloud-zone", default="us-central1-a",
                       help="GCP zone (default: us-central1-a)")
    cloud.add_argument("--cloud-machine", default="n2-highcpu-8",
                       help="GCP machine type (default: n2-highcpu-8)")
    cloud.add_argument("--cloud-spot", action="store_true", default=True,
                       help="Use spot/preemptible instance (default)")
    cloud.add_argument("--cloud-on-demand", action="store_true",
                       help="Use on-demand instance (for retries/hard instances)")
    cloud.add_argument("--cloud-max-hours", type=int, default=6,
                       help="Auto-delete safety cap in hours (default: 6)")
    cloud.add_argument("--cloud-parallelism", type=int, default=8,
                       help="Parallel solver count — 8 mimics competition (default: 8)")
    cloud.add_argument("--cloud-bucket", default=None,
                       help="GCS bucket for CNF files (optional, speeds up repeated runs)")
    cloud.add_argument("--cloud-project", default="spinsat",
                       help="GCP project ID (default: spinsat)")
    cloud.add_argument("--cloud-cleanup", action="store_true",
                       help="List/cleanup leftover benchmark instances")
    cloud.add_argument("--cloud-recover", metavar="INSTANCE",
                       help="Recover results from a running/stopped VM (e.g. spinsat-bench-20260322-085915)")
    cloud.add_argument("--dry-run", action="store_true",
                       help="Show what would happen without executing")
    cloud.add_argument("--retry-incomplete",
                       help="Path to a PARTIAL results JSON — re-run only missing instances")

    args = parser.parse_args()

    if args.list_suites:
        print("Available benchmark suites:")
        for name, suite in SUITES.items():
            print(f"  {name:10s} — {suite['description']}")
        return

    # Cloud cleanup mode
    if getattr(args, "cloud_cleanup", False):
        from cloud_benchmark import CloudBenchmark
        CloudBenchmark.cleanup_instances(project=args.cloud_project)
        return

    if getattr(args, "cloud_recover", None):
        from cloud_benchmark import CloudBenchmark
        cb = CloudBenchmark(
            zone=args.cloud_zone,
            project=args.cloud_project,
        )
        cb.instance_name = args.cloud_recover
        cb._instance_created = True
        run_id = datetime.now().strftime("%Y%m%d_%H%M%S")
        try:
            results = cb.recover_results(
                run_id=run_id,
                tag=args.tag or "recovered",
                timeout_s=args.timeout,
            )
            fpath = save_results(results, "recovered")
            print_summary(results)
            print(f"Results saved to: {fpath}")
            print(f"\nVM still running. Delete when done:")
            print(f"  gcloud compute instances delete {cb.instance_name} --zone {cb.zone} --project {cb.project}")
        except Exception as e:
            print(f"Recovery failed: {e}")
            print(f"VM: {cb.instance_name} (zone: {cb.zone})")
        return

    solvers = args.solvers or ["spinsat"]
    suite_name = args.suite or "custom"

    # Cloud mode only supports spinsat
    if args.cloud and solvers != ["spinsat"]:
        print("Cloud mode only supports the 'spinsat' solver.")
        return

    # Verify solvers exist (local mode only)
    if not args.cloud:
        for s in solvers:
            if s not in SOLVERS:
                print(f"Unknown solver: {s}")
                print(f"Available: {', '.join(SOLVERS.keys())}")
                return

    # Handle --retry-incomplete
    if args.retry_incomplete:
        with open(args.retry_incomplete) as f:
            partial = json.load(f)
        completed = {inst["instance"] for inst in partial.get("instances", [])}
        all_instances = collect_instances(args.suite, args.instances)
        instances = [p for p in all_instances if os.path.basename(p) not in completed]
        print(f"Retry mode: {len(completed)} complete, {len(instances)} remaining")
        if not instances:
            print("All instances already complete!")
            return
    else:
        # Collect instances
        instances = collect_instances(args.suite, args.instances)

    if not instances:
        print("No instances found. Use --suite or --instances.")
        return

    # Cloud execution path
    if args.cloud:
        cloud_run(args, instances, suite_name)
        return

    # --- Local execution path (unchanged) ---

    # Auto-detect metadata if recording
    run_metadata = None
    if args.record:
        if not BENCHMARKS_DB.exists():
            print(f"Error: {BENCHMARKS_DB} not found.")
            print("Run: python3 scripts/init_benchmarks_db.py")
            return

        spinsat_cmd = SOLVERS["spinsat"]["cmd"]
        solver_version = detect_solver_version(spinsat_cmd)
        git_commit, git_dirty = detect_git_info()

        if git_dirty and not args.force:
            print("Warning: uncommitted changes detected. Results may not be reproducible.")
            print("Use --force to record anyway, or commit changes first.")
            try:
                response = input("Continue recording? [y/N] ").strip().lower()
                if response != 'y':
                    print("Aborting.")
                    return
            except EOFError:
                print("\nAborting (non-interactive). Use --force to skip this check.")
                return

        # Reconstruct the CLI command for reproducibility
        cli_command = " ".join(sys.argv)

        run_metadata = {
            "run_id": datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S") + f"_{uuid.uuid4().hex[:6]}",
            "solver_version": solver_version,
            "git_commit": git_commit,
            "git_dirty": git_dirty,
            "hardware": detect_hardware(),
            "rust_version": detect_rust_version(),
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "notes": args.notes,
            "cli_command": cli_command,
            "tag_purpose": getattr(args, "purpose", None),
            "tag_instance_set": getattr(args, "instance_set", None),
            "tag_config": getattr(args, "config", None),
        }

        print("=" * 70)
        print("OFFICIAL BENCHMARK RUN")
        print("=" * 70)
        print(f"  Solver version: {solver_version}")
        print(f"  Git commit:     {git_commit}{'*' if git_dirty else ''}")
        print(f"  Hardware:       {run_metadata['hardware']}")
        print(f"  Rust:           {run_metadata['rust_version']}")
        print()

    print(f"Suite: {suite_name}")
    print(f"Instances: {len(instances)}")
    print(f"Solvers: {', '.join(solvers)}")
    print(f"Timeout: {args.timeout}s")
    print(f"Tag: {args.tag or '(none)'}")
    if args.record:
        print(f"Recording: YES → {BENCHMARKS_DB}")
    print()

    # Run benchmark
    results = run_benchmark(solvers, instances, args.timeout, args.tag)

    # Save JSON (always)
    fpath = save_results(results, suite_name)
    print_summary(results)
    print(f"Results saved to: {fpath}")

    # Record to DB (if --record)
    if args.record and run_metadata:
        # Update strategy/method/restart/preprocessing from first result's stderr parse
        for inst in results["instances"]:
            r = inst.get("spinsat", {})
            if r.get("strategy"):
                run_metadata["strategy"] = r["strategy"]
                run_metadata["integration_method"] = r.get("method_used")
                if r.get("restart_strategy"):
                    run_metadata["restart_strategy"] = r["restart_strategy"]
                if r.get("preprocessing"):
                    run_metadata["preprocessing"] = r["preprocessing"]
                break

        recorded = record_to_db(results, "spinsat", run_metadata)
        if recorded:
            print(f"Recorded {recorded} results to {BENCHMARKS_DB}")
            print(f"  Run ID: {run_metadata['run_id']}")
            print(f"  Version: {run_metadata['solver_version']} ({run_metadata['git_commit']})")


if __name__ == "__main__":
    main()
