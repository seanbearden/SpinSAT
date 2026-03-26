# PRD: Optuna-Based Experiment Framework for SpinSAT

## Problem Statement

SpinSAT needs a systematic way to tune solver parameters and evaluate integration/restart methods against SAT Competition 2025 instances. The current benchmarking infrastructure (`benchmark_suite.py`, `cloud_benchmark.py`) supports running experiments on GCP but lacks:

1. **Automated parameter tuning** — parameters are chosen manually or via brute-force grid search (`tune_restart_params.py`)
2. **Rich telemetry** — solver emits some data via stderr but we don't capture convergence trajectories or intermediate metrics
3. **Multi-VM scalability** — current setup runs one VM at a time; 399 instances × 5000s timeout = days on a single VM
4. **Structured experiment management** — no way to define, track, and compare experiment campaigns

## Goals

### G1: Optuna Integration for Parameter Tuning
- Integrate Optuna as the hyperparameter optimization engine
- Support all tunable solver parameters:
  - Continuous: α (1-20), β (5-100), γ (0.01-0.9), δ (0.001-0.2), ε (1e-4 to 1e-2), ζ (1e-3 to 1e-1, log scale)
  - Categorical: integration method (euler/rk4/trapezoid), restart strategy (cycling/cold/none), preprocessing (on/off)
  - Conditional: restart-specific params only when restarts enabled
- Use TPE sampler for intelligent exploration of the parameter space
- Support pruning of unpromising trials (SuccessiveHalving or Hyperband)
- Objective: minimize PAR-2 score across a representative instance subset

### G2: Distributed Execution on GCP
- Run Optuna trials across multiple spot VMs concurrently
- Use Cloud SQL (PostgreSQL) as shared Optuna RDB storage backend
- Each VM pulls trial parameters from the study, runs solver, reports results
- Spot VM preemption handled gracefully (trial marked as failed, retried)
- Cost target: stay under $50 for a full tuning campaign (~200 trials)

### G3: Rich Telemetry Collection
- Capture per-instance metrics beyond status/time:
  - `peak_xl_max` — max long-term memory (already captured)
  - `final_dt` — final adaptive timestep (already captured)
  - Restart count and restart trigger reasons
  - Convergence trajectory: periodic snapshots of (unsatisfied clauses, xl_max, dt) at fixed intervals
  - Wall clock and CPU time (already captured)
  - Memory high-water mark
- Store telemetry in benchmarks.db with new schema extensions
- Enable dashboard visualizations of convergence patterns

### G4: SAT 2025 Instance Benchmarking
- Use SAT Competition 2025 Main Track instances (399 instances, 5.3GB compressed)
- 5000s timeout per instance (competition standard)
- Gather competition reference data for SAT2025 instances (for head-to-head comparison)
- Record all results to benchmarks.db with proper structured tags

### G5: Experiment Campaign Management
- Define experiment campaigns as configuration files (YAML/TOML)
- Campaign specifies: instance set, parameter search space, timeout, budget, number of trials
- Track campaign state: which trials completed, budget consumed, current best
- Resume interrupted campaigns (Optuna's RDB storage enables this natively)

## Non-Goals

- Changing the core solver algorithm or ODE equations
- Multi-core parallelism within a single solver run (competition rules: single core)
- Building a web UI for experiment management (CLI + dashboard is sufficient)
- Supporting cloud providers other than GCP
- Real-time monitoring/alerting during experiments (check results after completion)
- Updating GCP spot pricing in the codebase (current pricing is fine)

## User Stories

### US1: Parameter Tuning Campaign
As a researcher, I want to run `python3 scripts/optuna_tune.py --campaign tuning.yaml` and have it:
1. Create/resume an Optuna study backed by Cloud SQL
2. Spin up N spot VMs on GCP
3. Each VM runs trials: pull params from study → run solver on instance subset → report PAR-2 → repeat
4. After budget exhausted (trials or $), collect results and show best parameters
5. Record best configuration results to benchmarks.db

### US2: Controlled Experiment
As a researcher, I want to compare integration methods (euler vs rk4 vs trapezoid) with fixed parameters across all SAT2025 instances:
1. Define 3 experiment arms in a campaign config
2. Run all arms on GCP with multi-VM parallelism
3. Results automatically recorded to benchmarks.db with structured tags
4. Dashboard shows head-to-head comparison

### US3: Convergence Analysis
As a researcher, I want to understand WHY certain instances are hard:
1. Run solver with trajectory logging enabled
2. Inspect convergence curves: xl_max over time, dt adaptation, clause satisfaction progress
3. Compare trajectories between solved and timed-out instances
4. Use insights to guide parameter tuning or algorithm changes

### US4: Budget-Conscious Experimentation
As a researcher, I want experiments to respect cost constraints:
1. Set a dollar budget per campaign
2. Framework estimates cost before starting and warns if budget likely exceeded
3. Auto-shutdown VMs when budget consumed or trials complete
4. Show actual cost vs estimated cost in results summary

## Technical Constraints

- **Competition target**: SAT Competition 2026 Experimental Track (single-core, 5000s timeout)
- **Solver binary**: Pre-compiled Rust static binary (x86_64-unknown-linux-musl)
- **GCP**: Spot VMs (n2-highcpu-8), us-central1 region, ~$0.10/hr
- **Storage**: benchmarks.db (SQLite, distributed via GitHub Releases), Cloud SQL for Optuna during experiments
- **Existing infra**: Must build on/extend benchmark_suite.py and cloud_benchmark.py, not replace them
- **Dashboard**: Static HTML (sql.js + Chart.js), deployed via GitHub Pages

## Open Questions

1. **Instance subset for tuning**: Should we tune on all 399 SAT2025 instances or a representative subset? A subset of ~50 would make trials 8x cheaper but might miss edge cases.
2. **Optuna objective**: PAR-2 across instances? Or multi-objective (solve-rate AND average time)?
3. **Convergence trajectory format**: How often to snapshot? Every N timesteps? Every T seconds? What's the storage overhead?
4. **Cloud SQL lifecycle**: Persistent instance (always-on, ~$7/mo) or spin up per campaign?
5. **Solver modifications needed**: Does the solver need new CLI flags to emit trajectory data, or can we capture enough from existing stderr?
6. **Multi-seed handling**: Should each Optuna trial run the same configuration with multiple seeds to account for solver stochasticity?

## Success Metrics

- **Tuning effectiveness**: Optuna finds configurations that improve PAR-2 by ≥10% over default parameters on SAT2025 instances
- **Cost efficiency**: Full tuning campaign (200 trials) completes for under $50
- **Data richness**: Every recorded result includes convergence trajectory data
- **Reproducibility**: Any experiment can be exactly reproduced from its campaign config + solver version
- **Dashboard integration**: New experiment results visible in dashboard within one `gh release upload` cycle
