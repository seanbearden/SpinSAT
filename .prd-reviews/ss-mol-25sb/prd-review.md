# PRD Review: Optuna Experiment Framework — Consolidated Findings

## Critical Issues (4)

### C1: Solver CLI lacks flags for most tunable parameters
The PRD lists alpha, beta, gamma, delta, epsilon as tunable. **None of these have CLI flags.** They are hardcoded in `Params::default()` (dmm.rs). Alpha is per-clause (`alpha_m`) with dynamic competition heuristic — "tune alpha" is ambiguous (initial value? multipliers? interval?). Adding CLI flags requires Rust changes and is a **hard prerequisite** for G1.

### C2: No MVP/phasing defined
Five goals presented as a flat list. G1 (local Optuna) delivers standalone value without G2 (multi-VM) or G3 (telemetry). Recommended phasing:
- **Phase 0**: Add CLI flags for ODE params (Rust)
- **Phase 1 (MVP)**: Local Optuna + SQLite storage, single machine, tune on instance subset
- **Phase 2**: Distributed GCP execution
- **Phase 3**: Rich telemetry / convergence trajectories
- **Phase 4**: Campaign management (G5, only if needed)

### C3: $50 budget for 200 trials is tight-to-infeasible
At 50 instances/trial × 5000s timeout × 8-core parallelism = ~3.5 wall-hours/trial. 200 trials × $0.10/hr = ~$70 minimum, before Cloud SQL, overhead, or preemption retries. With 20 instances it barely fits (~$45). Consider reduced timeout for tuning (300s) with full timeout for validation.

### C4: Cloud SQL adds significant operational burden
Requires IAM, VPC/Auth Proxy, connection management, monthly cost (~$7/mo), and a named operator. Alternatives: (a) coordinator pattern — one VM runs Optuna, dispatches via SSH (reuses existing infra), (b) Optuna JournalStorage on GCS, (c) SQLite on shared storage.

## Major Issues (10)

### M1: Parameter space incomplete and inconsistent with solver code
- Missing: `xl_decay` (0.0-0.9), `restart_noise` (0.01-0.5), `auto_zeta` mode
- Integration methods: PRD lists 3 (euler/rk4/trapezoid), solver has 6 (+ alternate, probe, auto)
- Restart modes: PRD lists 3 (cycling/cold/none), solver has 5 (+ warm, anti-phase; `--no-restart` is separate flag)

### M2: Multi-seed handling is a requirement, not an open question
Solver is stochastic. Without multi-seed trials, Optuna overfits to lucky seeds. But 3 seeds/trial triples cost. Must be decided upfront.

### M3: Cloud SQL → benchmarks.db bridge unspecified
How do Optuna trial results flow to SQLite? Manual export? Automatic? Intermediate trial data (200 trials, not just best) is scientifically valuable but lost when Cloud SQL torn down.

### M4: Multi-VM orchestration is architecturally new
Current `cloud_benchmark.py` manages exactly one VM. Multi-VM requires fleet creation, shared state, aggregated cost tracking, partial failure handling. "Extend, not replace" constraint may force awkward abstractions.

### M5: Convergence trajectory storage vs DB size constraint
benchmarks.db target is <100MB (currently ~26MB). Dashboard loads entire DB in browser. Trajectory time-series for thousands of runs could easily exceed this. Consider separate `trajectories.db`.

### M6: G3 telemetry requires solver modifications
Convergence snapshots (unsatisfied clauses, xl_max, dt over time) not in current stderr output. Existing `trace.rs` is compile-time gated, binary format, records different metrics. Simplest approach: periodic stderr lines parseable by Python.

### M7: SAT 2025 reference data may not exist yet
CLAUDE.md rule: "ALWAYS gather competition reference results BEFORE running SpinSAT." If SAT 2025 competition results aren't public, this blocks benchmarking. Need fallback plan (SAT 2022 overlap data?).

### M8: "Extend, not replace" constraint conflicts with architectural needs
Multi-VM Optuna with autonomous workers is fundamentally different from single-VM bash worker pattern. Python workers needed on VMs (to talk to Optuna storage), replacing `cloud_worker.sh`.

### M9: Dollar-based budgeting is harder than described
Requires GCP pricing API or hardcoded rates, cumulative tracking across VMs, coordination to stop fleet when budget hit. Trial-count + max-hours may be a sufficient proxy.

### M10: Existing tune_restart_params.py not referenced
Already does grid search over xl_decay/restart_noise with PAR-2. Should be explicitly deprecated or identified as predecessor.

## Minor Issues (7)
- Acceptance criteria partially measurable (trajectory data definition unclear)
- G4 is existing workflow, not a new goal
- Preprocessing has 6 techniques but PRD treats as binary on/off
- Campaign YAML schema not specified
- No data retention/cleanup policy
- Dashboard convergence visualizations unspecified
- Spot VM preemption detection mechanism unspecified
