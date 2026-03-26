# Design Document: Optuna Experiment Framework for SpinSAT

## Phasing

| Phase | Scope | Effort |
|-------|-------|--------|
| **Phase 0** | Rust CLI flags for all ODE params | ~1-2 days |
| **Phase 1 (MVP)** | Local Optuna tuning, SQLite storage, single machine | ~3-5 days |
| **Phase 2** | Multi-VM GCP with JournalStorage on GCS | Future |
| **Phase 3** | Convergence trajectory telemetry | Future |

---

## Phase 0: Rust CLI Flags (Prerequisite)

### New CLI Flags

| Flag | Type | Default | Source |
|------|------|---------|--------|
| `--beta` | f64 | 20.0 | `Params::default()` in `dmm.rs:18` |
| `--gamma` | f64 | 0.25 | `Params::default()` in `dmm.rs:19` |
| `--delta` | f64 | 0.05 | `Params::default()` in `dmm.rs:20` |
| `--epsilon` | f64 | 1e-3 | `Params::default()` in `dmm.rs:21` |
| `--alpha-initial` | f64 | 5.0 | Hardcoded in `DmmState::new()` at `dmm.rs:80` |
| `--alpha-up-mult` | f64 | 1.1 | Hardcoded in `adjust_alpha_m()` at `dmm.rs:250` |
| `--alpha-down-mult` | f64 | 0.9 | Hardcoded in `adjust_alpha_m()` at `dmm.rs:252` |
| `--alpha-interval` | f64 | 1e4 | Hardcoded in `post_step()` at `integrator.rs:70` |

### Rust Changes

1. Add `alpha_initial`, `alpha_up_mult`, `alpha_down_mult`, `alpha_interval` to `Params` struct
2. Thread through `main.rs` CLI parsing → `Params` → `DmmState::new()` / `adjust_alpha_m()` / `post_step()`
3. Update solver stderr output to emit all param values
4. Update `parse_spinsat_stderr()` in `benchmark_suite.py` to capture new params

---

## Phase 1: Local Optuna MVP

### Architecture

```
optuna_tune.py (coordinator + worker, single machine)
  │
  ├── Load campaign YAML
  ├── Create/resume Optuna Study (SQLite)
  ├── TPE Sampler + SuccessiveHalving Pruner
  │
  └── for each trial:
        suggest_params() → build_solver_cmd()
        → run solver × 50 instances × 5 seeds × 300s timeout
        → compute PAR-2 → report to Optuna
        → prune if unpromising (after 5+ instances)
```

### New Files

| File | Purpose |
|------|---------|
| `scripts/optuna_tune.py` | Main entry point (~400-500 lines) |
| `scripts/campaign_config.py` | Campaign YAML parser/validator |
| `scripts/migrate_benchmarks_db_optuna.py` | DB schema migration |
| `campaigns/tune_ode_full.yaml` | Full parameter space example |
| `campaigns/compare_strategies.yaml` | Strategy comparison example |
| `requirements-tune.txt` | `optuna>=3.0`, `pyyaml` |

### Modified Files

| File | Change |
|------|--------|
| `src/main.rs` | Add 8 CLI flags |
| `src/dmm.rs` | Parameterize Params struct + adjust_alpha_m |
| `src/integrator.rs` | Parameterize alpha_interval in post_step |
| `scripts/benchmark_suite.py` | Extend `run_solver()` with `extra_args` param; update `parse_spinsat_stderr()` |
| `scripts/init_benchmarks_db.py` | Add new columns to schema |

### Campaign YAML Schema

```yaml
study_name: "tune-ode-v0.5.0"

objective:
  metric: par2
  direction: minimize
  seeds: [42, 137, 271, 404, 999]
  timeout_s: 300                    # reduced for tuning

instances:
  patterns: ["benchmarks/competition/sat2025/*.cnf"]
  max_instances: 50

search_space:
  # Continuous ODE params
  alpha_initial: {type: float, low: 1.0, high: 20.0}
  alpha_up_mult: {type: float, low: 1.01, high: 1.5}
  alpha_down_mult: {type: float, low: 0.5, high: 0.99}
  alpha_interval: {type: float, low: 1000, high: 50000, log: true}
  beta: {type: float, low: 5.0, high: 100.0}
  gamma: {type: float, low: 0.01, high: 0.9}
  delta: {type: float, low: 0.001, high: 0.2}
  epsilon: {type: float, low: 1.0e-4, high: 1.0e-2, log: true}
  # Zeta (conditional on auto_zeta)
  auto_zeta: {type: categorical, choices: [true, false]}
  zeta: {type: float, low: 1e-3, high: 1e-1, log: true, condition: {auto_zeta: false}}
  # Strategy
  strategy: {type: categorical, choices: [euler, trapezoid, rk4, alternate, probe, auto]}
  # Restart (conditional on no_restart)
  no_restart: {type: categorical, choices: [true, false]}
  restart_mode: {type: categorical, choices: [cold, warm, anti-phase, cycling], condition: {no_restart: false}}
  xl_decay: {type: float, low: 0.0, high: 0.9, condition: {no_restart: false}}
  restart_noise: {type: float, low: 0.01, high: 0.5, condition: {no_restart: false}}
  # Preprocessing
  preprocess: {type: categorical, choices: [true, false]}

budget:
  n_trials: 200
  max_wall_hours: 24

sampler: {type: TPE, seed: 42}
pruner: {type: SuccessiveHalving, min_resource: 5, reduction_factor: 3}

storage:
  type: sqlite
  path: "optuna_studies/tune-ode-v0.5.0.db"

validation:
  timeout_s: 5000
  seeds: [42, 137, 271, 404, 999]
  record_to_db: true
```

### CLI Interface

```bash
# Dry run — validate config, estimate cost, print sample command
python3 scripts/optuna_tune.py --campaign campaigns/tune_ode_full.yaml --dry-run

# Execute tuning campaign
python3 scripts/optuna_tune.py --campaign campaigns/tune_ode_full.yaml

# Resume interrupted campaign (automatic — same study_name resumes)
python3 scripts/optuna_tune.py --campaign campaigns/tune_ode_full.yaml

# Override YAML fields without editing file
python3 scripts/optuna_tune.py --campaign campaigns/tune_ode_full.yaml \
  --override budget.n_trials=10 --override instances.max_instances=5

# Report results + generate visualizations
python3 scripts/optuna_tune.py --campaign campaigns/tune_ode_full.yaml --report

# Validate best config with full timeout, record to benchmarks.db
python3 scripts/optuna_tune.py --campaign campaigns/tune_ode_full.yaml --validate-best
```

### Parameter Mapping: Optuna Trial → Solver CLI

```python
def build_solver_cmd(trial_params, instance_path, seed, timeout):
    cmd = [SOLVER_CMD, "-t", str(timeout), "-s", str(seed)]

    # ODE params (Phase 0 flags)
    for param in ["alpha-initial", "alpha-up-mult", "alpha-down-mult",
                   "alpha-interval", "beta", "gamma", "delta", "epsilon"]:
        key = param.replace("-", "_")
        if key in trial_params:
            cmd.extend([f"--{param}", str(trial_params[key])])

    # Zeta / auto-zeta
    if trial_params.get("auto_zeta") is False:
        cmd.extend(["--zeta", str(trial_params["zeta"]), "--no-auto-zeta"])

    # Strategy
    if "strategy" in trial_params:
        cmd.extend(["--method", trial_params["strategy"]])

    # Restart
    if trial_params.get("no_restart"):
        cmd.append("--no-restart")
    else:
        if "restart_mode" in trial_params:
            cmd.extend(["--restart-mode", trial_params["restart_mode"]])
        if "xl_decay" in trial_params:
            cmd.extend(["--xl-decay", str(trial_params["xl_decay"])])
        if "restart_noise" in trial_params:
            cmd.extend(["--restart-noise", str(trial_params["restart_noise"])])

    # Preprocessing
    if trial_params.get("preprocess") is False:
        cmd.append("--no-preprocess")

    cmd.append(instance_path)
    return cmd
```

### Objective Function

```python
def objective(trial, config):
    params = suggest_params(trial, config.search_space)

    par2_total = 0.0
    instance_count = 0

    for i, instance in enumerate(config.instances):
        for seed in config.seeds:  # 5 seeds
            cmd = build_solver_cmd(params, instance, seed, config.timeout_s)
            result = run_solver(cmd)  # reuse from benchmark_suite.py

            if result["status"] == "SATISFIABLE":
                par2_total += result["time_s"]
            else:
                par2_total += 2 * config.timeout_s

        instance_count += 1
        # Report intermediate for pruning
        trial.report(par2_total / (instance_count * len(config.seeds)), step=i)
        if trial.should_prune():
            raise optuna.TrialPruned()

    return par2_total / (len(config.instances) * len(config.seeds))
```

### Schema Extensions for benchmarks.db

```sql
-- runs table additions
ALTER TABLE runs ADD COLUMN optuna_study TEXT;
ALTER TABLE runs ADD COLUMN optuna_trial_number INTEGER;

-- results table additions (many columns already exist but are NULL)
ALTER TABLE results ADD COLUMN xl_decay REAL;
ALTER TABLE results ADD COLUMN restart_noise REAL;
ALTER TABLE results ADD COLUMN alpha_initial REAL;
ALTER TABLE results ADD COLUMN alpha_up_mult REAL;
ALTER TABLE results ADD COLUMN alpha_down_mult REAL;
ALTER TABLE results ADD COLUMN alpha_interval REAL;
ALTER TABLE results ADD COLUMN restart_mode TEXT;
ALTER TABLE results ADD COLUMN strategy_used TEXT;
ALTER TABLE results ADD COLUMN preprocess_enabled INTEGER;
```

### Progress Reporting

```
[Trial 001/200] strategy=euler beta=15.3 gamma=0.42 restart=cycling
  Solved 34/40 instances (6 timeout), PAR-2=1847.2
  Current best: Trial 001 PAR-2=1847.2 | ETA: ~6.8h remaining

[Trial 002/200] strategy=rk4 beta=42.1 gamma=0.11 restart=cold
  PRUNED at instance 15/40 (PAR-2 trending worse than best)
  Current best: Trial 001 PAR-2=1847.2 | ETA: ~5.9h remaining
```

### Report Output

```
=== STUDY RESULTS: tune-ode-v0.5.0 (200 trials) ===
Best trial (#23): PAR-2 = 1204.5
  strategy: rk4, beta: 31.7, gamma: 0.18, delta: 0.032, epsilon: 2.1e-3
  alpha_initial: 5.0, alpha_up: 1.1, alpha_down: 0.9, interval: 10000
  auto_zeta: true, restart: cycling, xl_decay: 0.35, noise: 0.08

Parameter importance (fANOVA):
  strategy: 0.31  ████████████████
  gamma:    0.18  █████████
  beta:     0.14  ███████
  ...

Visualizations: optuna_studies/tune-ode-v0.5.0/
  optimization_history.html, param_importances.html, parallel_coordinate.html
```

### Integration with Existing Systems

- **benchmark_suite.py**: Import `run_solver`, `parse_spinsat_stderr`, `parse_solver_output`, `record_to_db`, `collect_instances`. Extend `run_solver()` with `extra_args` parameter (backward compatible).
- **Dashboard**: No new tab in Phase 1. Best-config results appear as normal runs via `--record --config optuna-best-{study}`. SQL Explorer works on new columns.
- **GitHub Releases**: Upload `benchmarks.db` (includes Optuna best-config results). Optionally upload `optuna_studies/{name}.db` for full trial history.
- **tune_restart_params.py**: Deprecate with notice pointing to `optuna_tune.py`. Include migration campaign YAML using Optuna's `GridSampler`.

---

## Phase 2: Multi-VM GCP (Future)

### Architecture

```
Coordinator (local or VM)
  │  optuna_tune.py --campaign ... --cloud --workers 3
  │
  │  Optuna Study (JournalStorage on GCS)
  │  gs://spinsat-optuna/study-{name}.journal
  │
  ├── SSH → Worker VM 0 (spot, n2-highcpu-8)
  ├── SSH → Worker VM 1 (spot, n2-highcpu-8)
  └── SSH → Worker VM 2 (spot, n2-highcpu-8)
       Each: optuna_worker.py runs trial, reports results
```

- Reuses `CloudBenchmark` from `cloud_benchmark.py` for VM lifecycle
- Workers: Python script replaces `cloud_worker.sh`, talks to GCS journal
- Preemption: SSH poll fails 3x → mark trial failed → spin replacement VM
- Safety: 3-layer shutdown (GCP max-run-duration + OS shutdown + coordinator watchdog)
- Cost estimate: ~$34 for 200 trials with pruning, ~$40 validation = ~$74 total

### GCS Permissions

- Coordinator: `storage.objectAdmin` on `gs://spinsat-optuna/`
- Workers: `storage.objectViewer` + `storage.objectCreator` (can read journal, write results, cannot delete)

---

## Phase 3: Convergence Telemetry (Future)

- Periodic stderr snapshots: `c SNAPSHOT wall=1.23 unsat=45 xl_max=12.5 dt=0.01 restarts=2`
- Parsed by `parse_spinsat_stderr()`, stored in separate `trajectories.db`
- Schema: `trajectory_runs` (FK to benchmarks.db) + `trajectory_snapshots` (time series)
- Dashboard: optional "Tuning" tab loading trajectories.db via sql.js

---

## Cost Model

| Scenario | Instances | Seeds | Timeout | Parallelism | Per-Trial | 200 Trials |
|----------|-----------|-------|---------|-------------|-----------|------------|
| Phase 1 local (8-core) | 50 | 5 | 300s | 8 | ~2.6h | ~520h local |
| Phase 2 cloud (3 VMs) | 50 | 5 | 300s | 24 | ~0.9h | ~$34 |
| Validation (best 5) | 399 | 5 | 5000s | 8 | ~17h | ~$40 total |

---

## Review Findings Applied

### Fixes from alignment/review rounds (steps 5-10)

1. **Smoke test campaign**: Phase 1 local = ~520 wall-hours for full 200 trials — impractical to validate. Added a smoke test campaign (10 instances, 3 seeds, 5 trials) that completes in ~30 min on an 8-core machine.

2. **Phase 0 verification**: Added explicit test commands and expected stderr format so Phase 0 can be verified before Phase 1 begins.

3. **Dropped `--override` from MVP**: Generic dotted-path YAML override parser is scope creep. Edit the YAML or create a small variant instead.

4. **`--report` deferred**: Optuna's built-in `optuna-dashboard` or Jupyter can serve this initially. Phase 1 MVP prints best trial to stdout. `--validate-best` kept (core workflow).

5. **PAR-2 handles UNSATISFIABLE correctly**: UNSAT results scored by actual time, not penalized as timeout.

6. **Migration script idempotent**: Check column existence via `PRAGMA table_info` before `ALTER TABLE`.

7. **Conditional param unit tests**: Added test specification for `build_solver_cmd()` covering all branches.

8. **Instance ordering for pruning**: Sort instances easy-to-hard (by median competition solve time) to improve SuccessiveHalving signal quality.

9. **Cost target revision**: PRD's $50 covers tuning only. Validation costs extra (~$40). Total campaign budget revised to ~$75.

10. **Competition reference data**: Added task to acquire SAT 2025 results (or use SAT 2022 overlap) before benchmarking.

11. **Memory high-water mark**: Added to Phase 3 snapshot format via `getrusage` maxrss in Python wrapper.

12. **DmmState::new() signature**: Phase 0 must thread `alpha_initial` from Params into `DmmState::new()`, updating call sites including ~7 test files.

### Phase 0 Verification Criteria

After completing Phase 0 Rust changes, verify with:
```bash
cargo build --release
./target/release/spinsat --beta 30 --gamma 0.1 --alpha-initial 3.0 \
  --alpha-up-mult 1.2 --alpha-down-mult 0.8 --alpha-interval 5000 \
  -t 60 -s 1 tests/test1.cnf 2>&1 | grep -E "beta|gamma|alpha"
```
Expected stderr must emit all param values so `parse_spinsat_stderr()` can capture them.

### Smoke Test Campaign

```yaml
# campaigns/smoke_test.yaml — validates Phase 1 end-to-end (~30 min)
study_name: "smoke-test"
objective: {metric: par2, direction: minimize, seeds: [42, 137, 271], timeout_s: 60}
instances: {patterns: ["tests/*.cnf"], max_instances: 10}
search_space:
  beta: {type: float, low: 10, high: 40}
  method: {type: categorical, choices: [euler, rk4]}
budget: {n_trials: 5, max_wall_hours: 1}
storage: {type: sqlite, path: "optuna_studies/smoke.db"}
```

### Unit Tests for build_solver_cmd()

Required test cases:
1. All params active (restart enabled, manual zeta, preprocessing on)
2. `no_restart: true` — no `--restart-mode`, `--xl-decay`, `--restart-noise` in cmd
3. `auto_zeta: true` — no `--zeta` or `--no-auto-zeta` in cmd
4. `preprocess: false` — `--no-preprocess` present
5. Verify UNSATISFIABLE instances scored by actual time (not 2x penalty)

---

## Deprecation

| Script | Status | Replacement |
|--------|--------|-------------|
| `tune_restart_params.py` | Deprecated after Phase 1 validated | `optuna_tune.py` |

---

## Implementation Sequence

```
Phase 0 (prerequisite):
  1. src/dmm.rs: Add alpha params to Params struct, update DmmState::new() signature
  2. src/main.rs: Add 8 CLI flags, thread to Params
  3. src/integrator.rs: Parameterize alpha_interval in post_step()
  4. Update ~7 test call sites for new DmmState::new() signature
  5. benchmark_suite.py: Update parse_spinsat_stderr + record_to_db
  6. cargo build --release + verify stderr output (see verification criteria above)

Phase 1 (MVP):
  7. scripts/campaign_config.py: YAML parser (hardcode TPE + SuccessiveHalving)
  8. scripts/optuna_tune.py: Core tuning loop + --dry-run + --validate-best
  9. scripts/migrate_benchmarks_db_optuna.py: Idempotent schema migration
  10. campaigns/smoke_test.yaml + campaigns/tune_ode_full.yaml
  11. benchmark_suite.py: Add run_solver_with_args() (new function, not modify existing)
  12. requirements-tune.txt: optuna>=3.0, pyyaml
  13. Run smoke test campaign to validate end-to-end
```
