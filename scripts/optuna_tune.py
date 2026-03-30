#!/usr/bin/env python3
"""
Optuna-based hyperparameter tuning for SpinSAT.

Phase 1 MVP: local execution with SQLite storage, TPE sampler,
SuccessiveHalving pruner, multi-seed PAR-2 objective.

Usage:
    python3 scripts/optuna_tune.py --campaign campaigns/smoke_test.yaml
    python3 scripts/optuna_tune.py --campaign campaigns/smoke_test.yaml --dry-run
    python3 scripts/optuna_tune.py --campaign campaigns/smoke_test.yaml --validate-best
"""

import argparse
import os
import sys
import time
from pathlib import Path

import optuna

# Add scripts/ to path for sibling imports
SCRIPT_DIR = Path(__file__).parent
PROJECT_ROOT = SCRIPT_DIR.parent
sys.path.insert(0, str(SCRIPT_DIR))

from benchmark_suite import run_solver_with_args, detect_solver_version, detect_git_info, compute_instance_hash
from campaign_config import load_campaign, print_summary, CampaignConfig

# Solver binary path
SOLVER_CMD = str(PROJECT_ROOT / "target" / "release" / "spinsat")


# ---------------------------------------------------------------------------
# Parameter suggestion → CLI args mapping
# ---------------------------------------------------------------------------

def suggest_params(trial, search_space):
    """Suggest parameters from Optuna trial based on search space config.

    Returns a dict of parameter name → suggested value.
    """
    params = {}
    for p in search_space:
        # Check condition: skip this param if condition not met
        if p.condition:
            cond_key = list(p.condition.keys())[0]
            cond_val = p.condition[cond_key]
            if params.get(cond_key) != cond_val:
                continue

        if p.type == "float":
            params[p.name] = trial.suggest_float(p.name, p.low, p.high, log=p.log)
        elif p.type == "int":
            params[p.name] = trial.suggest_int(p.name, int(p.low), int(p.high))
        elif p.type == "categorical":
            params[p.name] = trial.suggest_categorical(p.name, p.choices)

    return params


def build_solver_cmd(params, timeout, seed):
    """Build solver command list from suggested parameters.

    Returns list of strings suitable for run_solver_with_args().
    """
    cmd = [SOLVER_CMD, "-t", str(timeout), "-s", str(seed)]

    # Parameter name → CLI flag mapping
    flag_map = {
        "beta": "--beta",
        "gamma": "--gamma",
        "delta": "--delta",
        "epsilon": "--epsilon",
        "alpha_initial": "--alpha-initial",
        "alpha_up_mult": "--alpha-up-mult",
        "alpha_down_mult": "--alpha-down-mult",
        "alpha_interval": "--alpha-interval",
        "zeta": "--zeta",
        "xl_decay": "--xl-decay",
        "restart_noise": "--restart-noise",
    }

    # Numeric params
    for param_name, flag in flag_map.items():
        if param_name in params:
            cmd.extend([flag, str(params[param_name])])

    # Zeta: disable auto-zeta when manually set
    if "zeta" in params:
        cmd.append("--no-auto-zeta")

    # Strategy / method
    strategy = params.get("strategy") or params.get("method")
    if strategy:
        cmd.extend(["--method", str(strategy)])

    # Restart configuration
    if params.get("no_restart") is True:
        cmd.append("--no-restart")
    elif "restart_mode" in params:
        cmd.extend(["--restart-mode", str(params["restart_mode"])])

    # Preprocessing
    if params.get("preprocess") is False:
        cmd.append("--no-preprocess")

    # Auto-zeta
    if params.get("auto_zeta") is False and "zeta" not in params:
        cmd.append("--no-auto-zeta")

    return cmd


# ---------------------------------------------------------------------------
# Objective function
# ---------------------------------------------------------------------------

_benchmarks_db_url_override = None  # Set by main() from --db-url arg


def _get_benchmarks_db_conn():
    """Get a connection to the Cloud SQL spinsat_benchmarks database, if available.

    Connection sources (tried in order):
    1. --db-url CLI arg (rewritten to target spinsat_benchmarks DB)
    2. SPINSAT_DB_URL env var
    3. Local password file (dev machine only)
    """
    import psycopg2

    # 1. Derive from the Optuna --db-url (works on cloud VMs)
    if _benchmarks_db_url_override:
        try:
            # Replace the database name in the URL: optuna -> spinsat_benchmarks
            bench_url = _benchmarks_db_url_override.replace("/optuna", "/spinsat_benchmarks")
            # Also replace user if needed
            bench_url = bench_url.replace("optuna:", "benchmarks:")
            return psycopg2.connect(bench_url, connect_timeout=5)
        except Exception:
            pass

    # 2. SPINSAT_DB_URL env var
    db_url = os.environ.get("SPINSAT_DB_URL")
    if db_url:
        try:
            return psycopg2.connect(db_url, connect_timeout=5)
        except Exception:
            pass

    # 3. Local password file (dev machine)
    try:
        pw_file = PROJECT_ROOT / "optuna_studies" / ".db-password-spinsat-optuna"
        if pw_file.exists():
            pw = pw_file.read_text().strip()
            return psycopg2.connect(
                host="34.57.20.164", dbname="spinsat_benchmarks",
                user="benchmarks", password=pw, connect_timeout=5,
            )
    except Exception:
        pass

    return None


def _record_trial_results(study_name, trial_number, params, instance_results, config):
    """Record per-instance trial results to Cloud SQL benchmarks DB."""
    conn = _get_benchmarks_db_conn()
    if conn is None:
        return  # silently skip if no DB available

    try:
        cur = conn.cursor()
        run_id = f"optuna_{study_name}_trial{trial_number}"
        solver_version = detect_solver_version(SOLVER_CMD)
        git_commit, git_dirty = detect_git_info()
        timestamp = time.strftime("%Y-%m-%dT%H:%M:%S")

        # Build solver_args string from params
        cmd_parts = build_solver_cmd(params, config.timeout_s, seed=0)
        solver_args = " ".join(cmd_parts[1:])  # skip solver binary path

        cur.execute("""
            INSERT INTO runs (run_id, solver_version, git_commit, git_dirty,
                timestamp, timeout_s, tag, notes, cli_command)
            VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s)
            ON CONFLICT (run_id) DO NOTHING
        """, (run_id, solver_version, git_commit, git_dirty,
              timestamp, config.timeout_s,
              f"optuna-{study_name}-t{trial_number}",
              f"Optuna study={study_name} trial={trial_number}",
              solver_args))

        for inst_path, seed, result in instance_results:
            instance_hash = compute_instance_hash(inst_path)
            cur.execute("""
                INSERT INTO results (run_id, instance_hash, status, time_s,
                    restarts, seed, peak_xl_max, final_dt, wall_clock_s,
                    cpu_time_s, num_vars, num_clauses, beta, gamma, delta,
                    zeta, alpha_initial, alpha_up_mult, alpha_down_mult,
                    alpha_interval)
                VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s)
                ON CONFLICT DO NOTHING
            """, (run_id, instance_hash, result.get("status"),
                  result.get("time_s"), result.get("restarts"), seed,
                  result.get("peak_xl_max"), result.get("final_dt"),
                  result.get("wall_clock_s"), result.get("cpu_time_s"),
                  result.get("num_vars"), result.get("num_clauses"),
                  params.get("beta"), params.get("gamma"), params.get("delta"),
                  params.get("zeta"), params.get("alpha_initial"),
                  params.get("alpha_up_mult"), params.get("alpha_down_mult"),
                  params.get("alpha_interval")))

        conn.commit()
    except Exception as e:
        print(f"  Warning: failed to record trial results to benchmarks DB: {e}",
              file=sys.stderr)
        conn.rollback()
    finally:
        conn.close()


def make_objective(config):
    """Create the Optuna objective function closure.

    Runs multiple instances in parallel (up to n_parallel) to utilize
    multi-core VMs. Pruning still checks after each batch of instances.
    """
    from concurrent.futures import ThreadPoolExecutor, as_completed

    instances = config.resolved_instances
    seeds = config.seeds
    timeout = config.timeout_s
    # Run instances in parallel across all available cores
    n_parallel = max(1, os.cpu_count() or 1)

    def _run_instance(params, instance, seeds, timeout):
        """Run one instance across all seeds. Returns list of (instance, seed, result)."""
        results = []
        for seed in seeds:
            run_cmd = build_solver_cmd(params, timeout, seed)
            result = run_solver_with_args(run_cmd, instance, timeout + 10)
            results.append((instance, seed, result))
        return results

    def objective(trial):
        params = suggest_params(trial, config.search_space)

        par2_total = 0.0
        n_runs = 0
        instance_results = []

        # Process instances in parallel
        instances_done = 0
        with ThreadPoolExecutor(max_workers=n_parallel) as pool:
            futures = {
                pool.submit(_run_instance, params, inst, seeds, timeout): (i, inst)
                for i, inst in enumerate(instances)
            }
            for future in as_completed(futures):
                idx, inst = futures[future]
                for instance, seed, result in future.result():
                    if result.get("error"):
                        raise optuna.TrialPruned(f"Solver error: {result['error']}")

                    if result["status"] == "SATISFIABLE":
                        par2_total += result["time_s"]
                    elif result["status"] == "UNSATISFIABLE":
                        par2_total += result["time_s"]
                    else:
                        par2_total += 2 * timeout

                    instance_results.append((instance, seed, result))
                    n_runs += 1

                # Report after each instance completes (accurate count)
                instances_done += 1
                avg_so_far = par2_total / n_runs
                trial.report(avg_so_far, step=instances_done)
                if trial.should_prune():
                    pool.shutdown(wait=False, cancel_futures=True)
                    raise optuna.TrialPruned()

        # Trial completed (not pruned) — record per-instance results to Cloud SQL
        study_name = trial.study.study_name
        _record_trial_results(study_name, trial.number, params,
                              instance_results, config)

        return par2_total / n_runs

    return objective


# ---------------------------------------------------------------------------
# Study creation
# ---------------------------------------------------------------------------

def create_sampler(config, worker_id=None):
    """Create Optuna sampler from campaign config.

    When worker_id is provided (distributed mode), the sampler seed is
    offset by a hash of the worker_id. This ensures concurrent workers
    don't all suggest the same first params when the study is empty.
    """
    sc = config.sampler
    seed = sc.seed
    if seed is not None and worker_id:
        # Deterministic per-worker seed offset
        import hashlib
        offset = int(hashlib.md5(worker_id.encode()).hexdigest()[:8], 16) % 10000
        seed = seed + offset

    if sc.type == "TPE":
        return optuna.samplers.TPESampler(seed=seed)
    elif sc.type == "Random":
        return optuna.samplers.RandomSampler(seed=seed)
    elif sc.type == "CmaEs":
        return optuna.samplers.CmaEsSampler(seed=seed)
    elif sc.type == "Grid":
        raise ValueError("Grid sampler not yet supported in optuna_tune.py")
    return optuna.samplers.TPESampler(seed=seed)


def create_pruner(config):
    """Create Optuna pruner from campaign config."""
    pc = config.pruner
    if pc.type == "SuccessiveHalving":
        return optuna.pruners.SuccessiveHalvingPruner(
            min_resource=pc.min_resource,
            reduction_factor=pc.reduction_factor,
        )
    elif pc.type == "Hyperband":
        return optuna.pruners.HyperbandPruner(
            min_resource=pc.min_resource,
            reduction_factor=pc.reduction_factor,
        )
    elif pc.type == "Median" or pc.type == "MedianPruner":
        return optuna.pruners.MedianPruner(
            n_startup_trials=getattr(pc, 'n_startup_trials', 5),
            n_warmup_steps=getattr(pc, 'n_warmup_steps', 30),
        )
    elif pc.type == "NopPruner":
        return optuna.pruners.NopPruner()
    return optuna.pruners.NopPruner()


def create_storage(config, db_url_override=None):
    """Create Optuna storage backend.

    For PostgreSQL: uses RDBStorage with heartbeat for crash resilience.
    For SQLite: uses simple URL string (local only).
    """
    if db_url_override:
        # CLI override — always treat as PostgreSQL
        return optuna.storages.RDBStorage(
            url=db_url_override,
            heartbeat_interval=60,
            grace_period=120,
        )

    if config.storage.type == "postgresql":
        url = config.storage.url
        if not url:
            raise ValueError("storage.url required for postgresql storage type")
        return optuna.storages.RDBStorage(
            url=url,
            heartbeat_interval=60,
            grace_period=120,
        )

    # SQLite (default, local)
    storage_path = config.storage.path
    os.makedirs(os.path.dirname(storage_path) or ".", exist_ok=True)
    return f"sqlite:///{storage_path}"


def create_study(config, db_url_override=None, worker_id=None):
    """Create or load an Optuna study."""
    storage = create_storage(config, db_url_override)

    study = optuna.create_study(
        study_name=config.study_name,
        storage=storage,
        sampler=create_sampler(config, worker_id=worker_id),
        pruner=create_pruner(config),
        direction=config.direction,
        load_if_exists=True,
    )
    return study


# ---------------------------------------------------------------------------
# Progress callback
# ---------------------------------------------------------------------------

class ProgressCallback:
    """Log trial results as they complete."""

    def __init__(self, config):
        self.config = config
        self.start_time = time.time()

    def __call__(self, study, trial):
        elapsed = time.time() - self.start_time
        n_complete = len([t for t in study.trials
                         if t.state == optuna.trial.TrialState.COMPLETE])
        n_pruned = len([t for t in study.trials
                        if t.state == optuna.trial.TrialState.PRUNED])
        total = self.config.n_trials

        best_val = study.best_value if study.best_trial else float("inf")

        status = "PRUNED" if trial.state == optuna.trial.TrialState.PRUNED else "COMPLETE"
        val_str = f"{trial.value:.2f}" if trial.value is not None else "N/A"

        # Estimate remaining time
        if n_complete + n_pruned > 0:
            avg_time = elapsed / (n_complete + n_pruned)
            remaining = avg_time * (total - n_complete - n_pruned)
            eta_str = f"{remaining / 60:.0f}m"
        else:
            eta_str = "?"

        print(
            f"[Trial {n_complete + n_pruned:03d}/{total}] "
            f"PAR-2={val_str} ({status}) | "
            f"Best: {best_val:.2f} | "
            f"ETA: {eta_str}",
            file=sys.stderr,
        )


# ---------------------------------------------------------------------------
# Dry run
# ---------------------------------------------------------------------------

def dry_run(config):
    """Print campaign summary and validate solver availability."""
    print("=== DRY RUN ===\n")
    print_summary(config)

    # Check solver binary
    if not Path(SOLVER_CMD).exists():
        print(f"\n✗ Solver not found: {SOLVER_CMD}", file=sys.stderr)
        print("  Run: cargo build --release", file=sys.stderr)
        return False

    # Check solver version
    try:
        version = detect_solver_version(SOLVER_CMD)
        git_commit, git_dirty = detect_git_info()
        print(f"\nSolver: {SOLVER_CMD}")
        print(f"  Version: {version}")
        print(f"  Git: {git_commit}{' (dirty)' if git_dirty else ''}")
    except Exception as e:
        print(f"\n⚠ Could not detect solver version: {e}", file=sys.stderr)

    # Print sample trial command
    print("\nSample trial command:")
    sample_params = {}
    for p in config.search_space:
        if p.condition:
            continue  # skip conditional for sample
        if p.type == "float":
            sample_params[p.name] = (p.low + p.high) / 2
        elif p.type == "categorical":
            sample_params[p.name] = p.choices[0]
    cmd = build_solver_cmd(sample_params, config.timeout_s, seed=42)
    instance = config.resolved_instances[0] if config.resolved_instances else "<instance.cnf>"
    print(f"  {' '.join(cmd)} {instance}")

    print(f"\n✓ Dry run complete. Ready to execute.")
    return True


# ---------------------------------------------------------------------------
# Validate best
# ---------------------------------------------------------------------------

def validate_best(config, study):
    """Run the best trial configuration with full timeout and all seeds."""
    if not study.best_trial:
        print("No completed trials found.", file=sys.stderr)
        return

    best = study.best_trial
    print(f"\n=== VALIDATING BEST TRIAL #{best.number} (PAR-2={best.value:.2f}) ===\n")
    print("Best parameters:")
    for k, v in sorted(best.params.items()):
        print(f"  {k}: {v}")

    val_config = config.validation
    timeout = val_config.timeout_s if val_config else 5000
    seeds = val_config.seeds if val_config else config.seeds

    cmd = build_solver_cmd(best.params, timeout, seed=0)
    print(f"\nRunning on {len(config.resolved_instances)} instances "
          f"× {len(seeds)} seeds × {timeout}s timeout...")

    results = []
    for instance in config.resolved_instances:
        for seed in seeds:
            run_cmd = build_solver_cmd(best.params, timeout, seed)
            result = run_solver_with_args(run_cmd, instance, timeout + 10)
            result["instance"] = instance
            result["seed"] = seed
            results.append(result)

    # Compute PAR-2
    par2_total = 0.0
    solved = 0
    for r in results:
        if r["status"] in ("SATISFIABLE", "UNSATISFIABLE"):
            par2_total += r["time_s"]
            solved += 1
        else:
            par2_total += 2 * timeout

    avg_par2 = par2_total / len(results) if results else 0
    print(f"\nValidation PAR-2: {avg_par2:.2f}")
    print(f"Solved: {solved}/{len(results)} "
          f"({100 * solved / len(results):.1f}%)")

    # Print command for recording to benchmarks.db
    print(f"\nTo record these results to benchmarks.db:")
    param_args = []
    for k, v in sorted(best.params.items()):
        flag = {
            "beta": "--beta", "gamma": "--gamma", "delta": "--delta",
            "epsilon": "--epsilon", "alpha_initial": "--alpha-initial",
            "alpha_up_mult": "--alpha-up-mult", "alpha_down_mult": "--alpha-down-mult",
            "alpha_interval": "--alpha-interval", "zeta": "--zeta",
            "xl_decay": "--xl-decay", "restart_noise": "--restart-noise",
        }.get(k)
        if flag:
            param_args.extend([flag, str(v)])
    print(f"  (Use benchmark_suite.py --record with the solver args above)")


# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------

def print_report(study, config):
    """Print study results summary."""
    trials = [t for t in study.trials
              if t.state == optuna.trial.TrialState.COMPLETE]
    pruned = [t for t in study.trials
              if t.state == optuna.trial.TrialState.PRUNED]

    print(f"\n=== STUDY RESULTS: {config.study_name} ===")
    print(f"Completed: {len(trials)}, Pruned: {len(pruned)}, "
          f"Total: {len(trials) + len(pruned)}/{config.n_trials}")

    if not trials:
        print("No completed trials.")
        return

    best = study.best_trial
    print(f"\nBest trial #{best.number}: PAR-2 = {best.value:.2f}")
    for k, v in sorted(best.params.items()):
        if isinstance(v, float):
            print(f"  {k}: {v:.6g}")
        else:
            print(f"  {k}: {v}")

    # Parameter importance (if enough trials)
    if len(trials) >= 5:
        try:
            importances = optuna.importance.get_param_importances(study)
            print("\nParameter importance (fANOVA):")
            for name, imp in sorted(importances.items(),
                                     key=lambda x: -x[1]):
                bar = "█" * int(imp * 40)
                print(f"  {name:20s} {imp:.3f} {bar}")
        except Exception:
            pass  # fANOVA can fail with too few trials

    # Save visualizations
    study_dir = Path(config.storage.path).parent / config.study_name
    study_dir.mkdir(parents=True, exist_ok=True)
    try:
        fig = optuna.visualization.plot_optimization_history(study)
        fig.write_html(str(study_dir / "optimization_history.html"))
        fig = optuna.visualization.plot_param_importances(study)
        fig.write_html(str(study_dir / "param_importances.html"))
        fig = optuna.visualization.plot_parallel_coordinate(study)
        fig.write_html(str(study_dir / "parallel_coordinate.html"))
        print(f"\nVisualizations saved to: {study_dir}/")
    except Exception as e:
        print(f"\n(Visualizations skipped: {e})", file=sys.stderr)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="Optuna hyperparameter tuning for SpinSAT"
    )
    parser.add_argument("--campaign", required=True,
                        help="Path to campaign YAML file")
    parser.add_argument("--dry-run", action="store_true",
                        help="Validate config and print plan without executing")
    parser.add_argument("--validate-best", action="store_true",
                        help="Run best trial with full timeout and record results")
    parser.add_argument("--n-trials", type=int,
                        help="Override number of trials from YAML")
    parser.add_argument("--timeout", type=int,
                        help="Override per-instance timeout from YAML")
    parser.add_argument("--db-url", type=str, default=None,
                        help="PostgreSQL URL for distributed mode "
                             "(overrides YAML storage config)")
    parser.add_argument("--worker-id", type=str, default=None,
                        help="Worker identifier for distributed logging")

    # Cloud execution
    cloud = parser.add_argument_group("cloud execution (GCP)")
    cloud.add_argument("--cloud", action="store_true",
                       help="Run distributed on GCP spot VMs")
    cloud.add_argument("--cloud-workers", type=int, default=4,
                       help="Number of spot VM workers (default: 4)")
    cloud.add_argument("--cloud-zone", default="us-central1-a",
                       help="GCP zone")
    cloud.add_argument("--cloud-machine", default="c3-standard-4",
                       help="GCP machine type (default: c3-standard-4)")
    cloud.add_argument("--cloud-max-hours", type=int, default=12,
                       help="Max VM lifetime in hours (default: 12)")
    cloud.add_argument("--cloud-project", default="spinsat",
                       help="GCP project ID")
    cloud.add_argument("--cloud-bucket", default="spinsat-benchmarks",
                       help="GCS bucket for instance files")
    cloud.add_argument("--cloud-db-instance", default="spinsat-optuna",
                       help="Cloud SQL instance name")
    cloud.add_argument("--cloud-db-region", default="us-central1",
                       help="Cloud SQL region")
    cloud.add_argument("--cloud-status", action="store_true",
                       help="Check status of running cloud study")
    cloud.add_argument("--cloud-cleanup", action="store_true",
                       help="Delete cloud resources (VMs, optionally Cloud SQL)")

    args = parser.parse_args()

    # Load campaign
    try:
        config = load_campaign(args.campaign)
    except Exception as e:
        print(f"Error loading campaign: {e}", file=sys.stderr)
        sys.exit(1)

    # Apply overrides
    if args.n_trials:
        config.n_trials = args.n_trials
    if args.timeout:
        config.timeout_s = args.timeout

    # Dry run
    if args.dry_run:
        ok = dry_run(config)
        sys.exit(0 if ok else 1)

    # Check instances
    if not config.resolved_instances:
        print("Error: No instances matched patterns. Check your campaign YAML.",
              file=sys.stderr)
        sys.exit(1)

    # Cloud mode — delegate to cloud orchestrator
    if args.cloud:
        from cloud_optuna import CloudOptuna
        cloud = CloudOptuna(
            campaign_path=args.campaign,
            config=config,
            n_workers=args.cloud_workers,
            zone=args.cloud_zone,
            machine_type=args.cloud_machine,
            max_hours=args.cloud_max_hours,
            project=args.cloud_project,
            bucket=args.cloud_bucket,
            db_instance=args.cloud_db_instance,
            db_region=args.cloud_db_region,
        )
        if args.cloud_status:
            cloud.status()
        elif args.cloud_cleanup:
            cloud.cleanup()
        else:
            cloud.run()
        sys.exit(0)

    if args.cloud_status or args.cloud_cleanup:
        print("--cloud-status and --cloud-cleanup require --cloud", file=sys.stderr)
        sys.exit(1)

    # Create study (with optional DB URL override for distributed workers)
    global _benchmarks_db_url_override
    if args.db_url:
        _benchmarks_db_url_override = args.db_url
    study = create_study(config, db_url_override=args.db_url, worker_id=args.worker_id)
    n_existing = len(study.trials)
    worker_tag = f" (worker={args.worker_id})" if args.worker_id else ""
    if n_existing > 0:
        print(f"Resuming study '{config.study_name}' with {n_existing} existing trials.{worker_tag}",
              file=sys.stderr)

    # Validate best mode
    if args.validate_best:
        validate_best(config, study)
        sys.exit(0)

    # Run optimization
    n_remaining = config.n_trials - n_existing
    if n_remaining <= 0:
        print(f"Study already has {n_existing} trials (budget: {config.n_trials}). "
              f"Use --validate-best to evaluate the best config.", file=sys.stderr)
    else:
        print(f"\nStarting optimization: {n_remaining} trials on "
              f"{len(config.resolved_instances)} instances × "
              f"{len(config.seeds)} seeds × {config.timeout_s}s timeout",
              file=sys.stderr)

        objective = make_objective(config)
        callback = ProgressCallback(config)
        callbacks = [callback]

        # Note: RetryFailedTrialCallback removed — it caused retry loops where
        # pruned trials created new failed entries that got retried with the same
        # params, starving TPE of exploration. Preempted workers are handled by
        # RDBStorage heartbeat/grace_period (marks trials as FAIL after grace period,
        # TPE samples new params for the next trial).

        study.optimize(
            objective,
            n_trials=n_remaining,
            callbacks=callbacks,
            show_progress_bar=False,
        )

    # Print report
    print_report(study, config)


if __name__ == "__main__":
    main()
