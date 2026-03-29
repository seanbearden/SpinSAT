# PRD: Continuous Cloud Benchmarking & Tuning Pipeline for SAT Competition 2026

## Problem Statement

SpinSAT is preparing for the SAT Competition 2026. We have a working solver, distributed Optuna tuning, and a local SQLite-based benchmarking system. But the current setup has critical gaps that prevent systematic competition preparation:

1. **Fragmented data**: Results live in a local SQLite file (`benchmarks.db`) that must be manually uploaded to GitHub Releases. There's no single source of truth accessible from cloud workers, local dev, and the dashboard simultaneously.

2. **No continuous benchmarking**: Benchmark runs are manual, one-off affairs. There's no system that continuously grinds through competition instances, building up coverage over time. With ~5K+ competition instances and 5000s timeouts, single-shot runs can't cover the space.

3. **Per-family tuning is manual**: Optuna tuning campaigns run against hand-picked instance subsets. There's no systematic approach to discovering which parameter configurations work best for which instance families (barthel, komb, qhid, structured, application, etc.).

4. **No instance-aware solver**: The solver uses fixed parameters regardless of instance characteristics. Competition solvers like Kissat adapt strategy based on instance features. SpinSAT needs a classification layer that selects parameters based on (vars, clauses, ratio, family, structure).

**Who**: SpinSAT developers (primarily Sean). The solver itself at competition time.

**Why now**: SAT Competition 2026 deadline approaching. We have the distributed infra (Cloud SQL, spot VMs, MIG, GCS bucket) but need to operationalize it into a continuous pipeline rather than ad-hoc runs.

## Goals

### G1: Cloud PostgreSQL as Central Results Database
- Migrate benchmark results storage from local SQLite to Cloud SQL PostgreSQL (reuse existing `spinsat-optuna` instance or create dedicated DB)
- Schema supports: solver runs, per-instance results, competition reference data, instance metadata, Optuna study results — all in one place
- Both cloud workers and local dev can read/write results
- Dashboard reads from Cloud SQL instead of downloading SQLite from GitHub Releases
- Maintain backward compatibility: local SQLite export for offline analysis and GitHub Pages fallback

### G2: Continuous Spot VM Benchmarking
- Always-on (or scheduled) spot VM pool that continuously runs SpinSAT against competition instances
- Work queue pattern: instances are queued, workers pull next unsolved instance, record result
- Progressive difficulty: start with instances where competition solvers are fast (easy), work toward harder ones
- Preemption-resilient: spot VM dies → instance goes back in queue, partial work isn't lost
- Budget controls: daily/weekly spend caps, auto-scale to 0 when budget exhausted
- Target: cover all ~5K anni2022 instances + 399 sat2025 instances within reasonable budget

### G3: Per-Family Optuna Tuning Campaigns
- Automated campaign scheduler that runs Optuna studies grouped by instance family
- Family detection: classify instances by source (barthel, komb, qhid) and structural features (vars, clauses, ratio, modularity)
- Each family gets its own Optuna study with TPE optimization
- Results feed back into a parameter lookup table: family → best parameters
- Campaign orchestration: rotate through families, allocate more budget to families where SpinSAT is weakest vs competition

### G4: Instance-Feature-Based Heuristic Selection
- Build a classification model from accumulated benchmark data
- Input features: num_vars, num_clauses, clause_var_ratio, max_clause_width, family (if known), structural features from GBD
- Output: recommended solver parameters (alpha, beta, gamma, delta, zeta, method, restart strategy, preprocessing)
- Integration into solver: at startup, solver reads instance features and selects parameter profile
- Fallback: if classification confidence is low, use default parameters
- Validation: compare auto-tuned solver vs fixed-parameter solver on held-out instances

### G5: Unified Dashboard & Monitoring
- Dashboard shows real-time progress: instances solved, current PAR-2, comparison to competition
- Per-family drill-down: which families are we competitive on, which need more tuning
- Cost tracking: GCP spend per campaign, cost per instance solved
- Alerting: notify when a campaign finishes, when budget is low, when a new best is found

## Non-Goals

- **UNSAT proof certificates**: Competition experimental track doesn't require them. No investment here.
- **Multi-core solving**: Competition rules require single-core. Pipeline is single-core per instance.
- **Solver algorithm changes**: This PRD is about the benchmarking/tuning infrastructure, not the ODE solver itself. Algorithm improvements (attention mechanisms, anti-phase restarts) are separate beads.
- **Real-time serving**: The heuristic selection is compile-time or startup-time, not a live inference service.
- **Other cloud providers**: GCP only. No AWS/Azure abstraction layer.
- **Dashboard redesign**: Extend existing dashboard, don't rewrite it.
- **Replacing Optuna**: Optuna stays as the HPO engine. No evaluation of alternatives.

## User Stories / Scenarios

### S1: Overnight Benchmarking Run
Sean kicks off a continuous benchmarking campaign before bed. Spot VMs pull instances from the work queue, solve them, record results to Cloud SQL. By morning, 500+ new instance results are available. Dashboard shows updated PAR-2 and coverage. If VMs were preempted, they auto-replaced and resumed from the queue.

### S2: Per-Family Tuning Campaign
Sean notices SpinSAT is slow on `qhid` instances. He launches an Optuna campaign targeting just qhid instances. After 100 trials, Optuna finds that qhid benefits from higher beta (29.6) and disabled preprocessing. These parameters are saved to the family parameter table. Next benchmark run uses them automatically for qhid instances.

### S3: Competition Submission Preparation
Two weeks before competition deadline. Sean runs the full solver against all benchmark instances using instance-feature heuristics. Dashboard shows head-to-head comparison against best competition solvers. For families where SpinSAT wins, parameters are locked. For families where it loses, targeted tuning campaigns run. Final parameter table is compiled into the solver binary.

### S4: New Solver Version Regression Test
After implementing a solver improvement (e.g., new restart strategy), Sean triggers a benchmark run of the new version against the same instance set. Dashboard shows version comparison: which instances got faster, which got slower, overall PAR-2 delta.

### S5: Cost-Aware Campaign Management
Budget is $100/month for cloud benchmarking. The system tracks spend and auto-scales workers to stay within budget. When approaching the limit, it prioritizes instances with highest information value (unsolved instances where competition solvers succeed, instances near the solve/timeout boundary).

## Constraints

### Technical
- **Cloud SQL already exists**: `spinsat-optuna` on `34.57.20.164`, db-g1-small, max_connections=100. Reuse or create new DB on same instance.
- **Pre-baked VM image exists**: `spinsat-optuna-worker` with solver, Python, Optuna pre-installed.
- **Solver is Rust**: Competition binary is static musl. No Python runtime on competition hardware.
- **Heuristic must be compiled in**: The instance classification can be trained in Python but must be exported as a lookup table or decision tree that Rust code can evaluate at startup.
- **Single-core constraint**: Each instance runs on one core. VMs can run multiple instances in parallel (different cores), but each individual solve is single-threaded.
- **Competition format**: DIMACS CNF input, standard SAT output format. No changes to I/O.

### Resource
- **Budget**: ~$100-200/month for cloud resources. Spot VMs are ~$0.01-0.03/hr depending on machine type. Cloud SQL db-g1-small is ~$0.03/hr (~$25/month always-on).
- **Timeline**: SAT Competition 2026 (submission deadline TBD, typically ~June). Need core pipeline working by April 2026, tuning campaigns running May-June.
- **Team**: One developer (Sean). Pipeline must be low-maintenance once running.

### Data
- **Instance corpus**: ~5K instances from anni2022, 399 from sat2025, plus historical sets (barthel, komb, qhid from sat2017/2018).
- **Competition reference data**: 150K rows in `competition_archive.db` (28 solvers x 5355 instances from anni2022).
- **Instance metadata**: 31K rows in `instances` table from GBD (Global Benchmark Database).

## Open Questions

1. **Cloud SQL vs. dedicated DB?** Reuse the existing `spinsat-optuna` PostgreSQL instance (add new databases/schemas) or provision a separate instance? Cost vs. isolation tradeoff.

2. **Work queue implementation?** Options: (a) PostgreSQL table with row-level locking (simple, no new infra), (b) Cloud Tasks / Pub/Sub (more robust, more complexity), (c) Redis queue. Leaning toward (a) for simplicity.

3. **Dashboard hosting?** Current dashboard is GitHub Pages (static, loads SQLite via sql.js). Options: (a) Keep static but add Cloud SQL export step, (b) Move to Cloud Run with server-side SQL, (c) Hybrid — static dashboard with periodic SQLite snapshots from Cloud SQL. Option (c) seems most practical.

4. **Heuristic model complexity?** Simple decision tree (clause_var_ratio thresholds → parameter set) vs. trained model (random forest, XGBoost on instance features). Decision tree is easier to compile into Rust. Trained model needs a serialization/codegen step.

5. **Instance family taxonomy?** Current families come from competition track names (barthel, komb, qhid). Do we need finer-grained clustering based on structural features? GBD provides modularity, treewidth, etc.

6. **Budget allocation strategy?** Fixed split (50% benchmarking, 50% tuning) vs. adaptive (more tuning when we're losing, more benchmarking when we need coverage)?

7. **How to handle sat2025 instances?** We don't have competition reference results for sat2025 yet (competition hasn't happened). Use sat2025 for absolute timing only, or train a difficulty estimator from anni2022 data?

8. **Optuna study management?** One study per family? One study per (family, method) pair? How to handle the combinatorial explosion of study dimensions?

9. **Data migration path?** How to migrate existing SQLite benchmarks.db data into Cloud SQL without losing history? One-time migration script + dual-write period?

10. **Monitoring and alerting?** GCP Cloud Monitoring + custom metrics, or simpler approach (periodic script that checks progress and sends Slack/email)?

## Rough Approach

### Phase 1: Cloud Database Foundation
- Create a `spinsat_benchmarks` database on the existing Cloud SQL instance
- Design PostgreSQL schema (mirror SQLite schema + work queue table + family parameters table)
- Write migration script: SQLite → PostgreSQL (one-time)
- Update `benchmark_suite.py` to support `--db-url` for PostgreSQL recording
- Keep SQLite as local cache / offline fallback

### Phase 2: Continuous Benchmarking Workers
- Extend the MIG/spot VM infrastructure (already in Terraform)
- Worker script: connect to Cloud SQL → pull next unprocessed instance from queue → run solver → record result → repeat
- Work queue table in PostgreSQL: instance_hash, status (pending/running/done/failed), worker_id, started_at, completed_at
- Progressive difficulty: sort queue by competition best time ascending (easy first)
- Budget controls: worker checks spend counter before starting next instance

### Phase 3: Systematic Tuning Campaigns
- Extend `optuna_tune.py` with family-aware campaign mode
- Campaign config: target family, instance subset, trial budget, parameter bounds
- Campaign scheduler: rotates through families based on performance gap vs competition
- Results stored in Cloud SQL alongside benchmark results
- Family parameter table: family → best trial parameters (auto-updated after each campaign)

### Phase 4: Instance Classification & Heuristic
- Feature extraction from DIMACS headers + GBD metadata
- Train simple classifier (decision tree / random forest) on (features → best parameter set)
- Export as Rust lookup table or embedded decision tree
- Integrate into solver: `--auto-tune` flag reads instance, selects parameters
- Validate on held-out instances, compare PAR-2 vs fixed parameters

### Phase 5: Dashboard & Monitoring
- Add Cloud SQL data source to dashboard (periodic export or direct connection)
- Per-family performance views
- Cost tracking integration
- Campaign progress monitoring

---

## Clarifications from Human Review

**Q1: What is the single monthly cloud budget number?**
A: $150/month is acceptable (including ~$25 Cloud SQL).

**Q2: What percentage of anni2022 instances must be attempted before competition?**
A: Focus on all 5K anni2022 instances over time. Strategy: do an initial sweep with 60s timeout to identify which instances are hard. Also test SAT Competition 2025 instances.

**Q3: Have the 2026 Experimental Track rules been confirmed?**
A: Verified online (March 2026). Key findings:
- **Experimental Track confirmed** for SAT Competition 2026
- No certificate requirement for Experimental Track (critical for SpinSAT — we can't produce UNSAT proofs)
- Must outperform top 3 Main Track solvers for an award
- Hardware: HoreKa Blue Instance (NHR@KIT), Intel Xeon Platinum 8368
- Time limit: 5000s, Memory limit: 32GB, 8 benchmarks run in parallel
- AI-Generated/AI-Tuned sub-tracks exist (separate category, not eligible for regular prizes)
- **Deadlines**: Solver registration April 19, Solver submission April 26, Documentation May 17
- Source code must be available (research license)
- System description (1-2 pages, IEEE Proceedings format) required

**Q4: Should ML-based heuristics be deferred?**
A: Do NOT defer ML heuristics, but it can be planned separately (separate bead/effort from core pipeline).

**Q5: Should benchmark and Optuna workers run concurrently?**
A: Yes, must support concurrent workloads. Implication: need connection budgeting and potentially Cloud SQL tier upgrade from db-g1-small.

**Q6: Family taxonomy?**
A: GBD `family` field is acceptable as the canonical family label.

**Q7: Sat2025 instances — include without reference data?**
A: Search for reference data online. Sean will also look. If not found, include instances anyway.

**Research finding on SAT 2025 reference data**: Per-instance solver runtime data for SAT 2025 is NOT yet publicly available as downloadable CSV/DB. The [benchmark-database.de](https://benchmark-database.de) has instances only (no runtimes). The [SAT Competition 2025 results page](https://satcompetition.github.io/2025/results.html) shows only aggregated leaderboard data. The [SAT Competition 2025 proceedings](https://repositum.tuwien.at/bitstream/20.500.12708/218424/2/Codel-2025-Proceedings%20of%20SAT%20Competition%202025%20%20Solver%20and%20Benchmark%20Desc...-vor.pdf) may contain some data. Solver binaries are downloadable from the [2025 downloads page](https://satcompetition.github.io/2025/downloads.html) — we could run them ourselves against sat2025 instances to generate reference times.

---

## Revised Scope Based on Review + Clarifications

### Competition MVP (must ship by April 19 solver registration)
1. Continuous benchmarking workers with PostgreSQL work queue
2. 60s-timeout sweep of all anni2022 instances to classify difficulty
3. Manual Optuna campaigns for weak families
4. Hardcoded parameter lookup table in Rust `main.rs` (based on tuning results)
5. Enable Cloud SQL backups + deletion_protection
6. Connection budgeting for concurrent workloads

### Planned Separately (own bead, not blocking competition)
- ML-based instance classification heuristic (Phase 4)

### Post-Competition
- Full SQLite-to-PostgreSQL migration
- Dashboard enhancements (real-time, cost tracking, alerting)
- Python-to-Rust codegen pipeline for decision trees
