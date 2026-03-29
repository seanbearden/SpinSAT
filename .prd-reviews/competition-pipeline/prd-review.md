# PRD Review Synthesis: Continuous Cloud Benchmarking & Tuning Pipeline

## Review Summary

Six dimensions reviewed by three parallel agents. **30 findings total**: 8 CRITICAL, 17 IMPORTANT, 5 NICE-TO-HAVE.

The dominant theme: **this PRD describes a platform, not a competition prep effort.** With one developer and ~2.5 months until a June 2026 deadline, the 5-phase plan is unrealistic. The reviewers converge on a much smaller MVP.

---

## CRITICAL Findings (Must Address Before Planning)

### C1: Five-phase plan exceeds timeline and team capacity
*[Scope review]*

All five phases cannot ship before June with one developer. Phase 4 (instance classification) is a research project requiring data from Phases 2-3 that won't exist in time. Phase 1 (full Cloud SQL migration) is high-risk, low-reward when the existing SQLite + GCS pipeline already works. Phase 5 (dashboard + monitoring + alerting) is unbounded.

**Recommendation**: Define a hard 2-tier plan: (a) Competition MVP (ship in 3-4 weeks), (b) Post-competition platform. The MVP is: continuous benchmarking workers + manual Optuna campaigns + hardcoded parameter table in Rust.

### C2: Budget is contradictory and unmeasurable
*[Requirements + Ambiguity reviews]*

Three different budget numbers appear: "reasonable budget" (G2), "$100-200/month" (Constraints), "$100/month" (S5). Cloud SQL alone costs ~$25/month, leaving $75-175 for compute. With 5K instances at 5000s timeout, worst-case is ~6,944 CPU-hours ($70-210 on spot VMs). Budget may be inconsistent with coverage targets.

**Recommendation**: Pick one number. Break it down: Cloud SQL fixed cost, max spot VM budget, Optuna allocation. Do the math showing whether full coverage is achievable.

### C3: Cloud SQL max_connections=100 will be exhausted
*[Gaps + Feasibility reviews]*

`db-g1-small` has 0.6 GB RAM and `max_connections=100`. Optuna workers use 2-4 connections each (SQLAlchemy pool). Benchmark workers need connections for queue polling. With 10+ concurrent VMs, plus dashboard, plus local dev, connection starvation is likely.

**Recommendation**: Add explicit connection budgeting. Require pool size of 2 per worker. Consider PgBouncer or documenting that benchmark and Optuna workers must not run simultaneously. Or upgrade the Cloud SQL tier.

### C4: No work queue concurrency/reaper strategy
*[Gaps + Feasibility reviews]*

PostgreSQL work queue (Option A) has no mechanism for: double-dispatch prevention (need `SELECT ... FOR UPDATE SKIP LOCKED`), zombie row reclamation (preempted VMs leave rows stuck in "running"), or priority ordering. No heartbeat mechanism exists.

**Recommendation**: Specify: workers claim rows with `FOR UPDATE SKIP LOCKED`. Workers update `heartbeat_at` every 60s. Reaper resets rows where `status='running' AND heartbeat_at < NOW() - 5min` to `pending`. Add a `priority` column.

### C5: No backup/DR for Cloud SQL — backups explicitly disabled
*[Gaps review]*

Terraform sets `backup_configuration { enabled = false }`. Making Cloud SQL the single source of truth without backups is a regression from the current system (SQLite on GitHub Releases). DB corruption or accidental DROP TABLE loses everything.

**Recommendation**: Enable daily automated backups (7-day retention). Set `deletion_protection = true`. Weekly pg_dump to GCS. Periodic SQLite snapshot as tertiary backup.

### C6: Solver binary's parameter consumption interface is undefined
*[Stakeholders review]*

G4 says "solver reads instance features and selects parameter profile at startup." But: no `--auto-tune` flag exists, no lookup table reader exists, no feature extractor runs during DIMACS parsing, and the competition `run.sh` interface has no mechanism to pass a parameter table. The table must be baked into the binary.

**Recommendation**: Define the exact contract: lookup table is a Rust `const` array compiled into the binary, indexed by clause-to-variable ratio bucket. `run.sh` passes no extra arguments. This forces the format decision now.

### C7: Phase 4 is research masquerading as engineering
*[Scope review]*

Instance classification requires: sufficient benchmark coverage (Phase 2), sufficient tuning data (Phase 3), feature extraction in Rust (doesn't exist), and a Python-to-Rust codegen pipeline (doesn't exist). Hard prerequisites that won't be met before competition.

**Recommendation**: Defer ML-based heuristic for 2027. For 2026, manually construct a 3-5 entry `match` statement on clause-to-variable ratio in `main.rs`. Takes 30 minutes, not weeks.

### C8: Migration correctness has no acceptance criteria
*[Requirements review]*

No specification for verifying data migrated correctly. No row-count reconciliation across 6 tables, no dual-write period defined, no feature flag for backend switching.

**Recommendation**: If migration is in scope (it probably shouldn't be for MVP): row-count match across all tables, dual-write for 2 weeks, `--db-backend` flag with `sqlite|postgres|both` values. Reverse export script tested before migration.

---

## IMPORTANT Findings (Address During Planning)

### I1: No security model for Cloud SQL credentials
Credentials embedded in VM startup scripts. No Cloud SQL Auth Proxy for local dev. No read-only dashboard user. No secret rotation.

### I2: No rollback plan for SQLite-to-PostgreSQL migration
One-time migration with no reverse path. No dual-write period. No backend switching flag.

### I3: No handling for solver crashes, OOM, or corrupted results
Only preemption is addressed. OOM kills (exit 137), segfaults (exit 139) leave queue rows stuck. No retry limits. No solution verification for SATISFIABLE results.

### I4: "Low-maintenance" undefined
No measurable SLA: how many days unattended? How many hours/week of ops work? Which failures must be self-healing?

### I5: Progressive difficulty undefined for sat2025 instances
399 sat2025 instances have no competition reference times. Cannot sort by "competition best time ascending." Need an explicit ordering policy.

### I6: "Family" definition is overloaded
Used to mean: source origin (barthel), competition track (main/random), and structural cluster. The parameter lookup table key depends on which definition is canonical.

### I7: Heuristic export format unresolved (lookup table vs. decision tree)
Two fundamentally different integration patterns with no decision criterion. Could lead to incompatible implementations.

### I8: Campaign completion criteria absent
No convergence criterion for Optuna studies. No minimum trial count. No definition of "weakest family."

### I9: Heuristic validation lacks performance target
No PAR-2 improvement threshold for accepting/rejecting the classification approach.

### I10: Spot preemption wastes long-running solves
A solver killed at 4500s of a 5000s run loses all work. No checkpointing exists. Budget waste should be quantified.

### I11: SQLite-to-PostgreSQL schema mismatch risk
SQLite uses TEXT types pervasively, allows implicit coercion. `instances` table schema is unstable (copied from external `meta.db`). Need schema audit before migration.

### I12: GCP billing/quota not operationalized
No `gcloud billing budgets create` configuration. No hard cap. No auto-shutdown trigger.

### I13: SAT Competition 2026 rules assumed, not verified
Experimental Track existence assumed. UNSAT certificate non-requirement assumed. Rules page should be checked before infrastructure work begins.

### I14: Optuna + benchmarking share Cloud SQL instance — hidden coupling
Connection exhaustion risk. If Cloud SQL goes down during benchmark, results lost (workers have no local buffer).

---

## NICE-TO-HAVE Findings

- N1: Dashboard "real-time" undefined (5-minute refresh is fine)
- N2: No documentation/runbook for pipeline operations
- N3: No architectural overview for future contributors
- N4: Work queue open question should be closed (PostgreSQL is clearly correct)
- N5: Phase 1 Cloud SQL migration is high-risk, low-reward for timeline

---

## Recommended MVP for Competition 2026

All three review agents converge on the same reduced scope:

| # | Deliverable | Effort | Dependencies |
|---|------------|--------|--------------|
| 0 | **Verify 2026 competition rules** | 1 hour | None |
| 1 | **PostgreSQL work queue table** on existing `spinsat-optuna` instance | 1-2 days | None |
| 2 | **Continuous benchmark worker script** (pull from queue, run solver, write results) | 3-5 days | #1 |
| 3 | **Manual Optuna campaigns** for 3-4 weakest families (already works via `optuna_tune.py --cloud`) | Ongoing | Existing infra |
| 4 | **Hardcoded parameter table** in `main.rs` as `match` on clause-to-variable ratio | 1 day | Benchmark data from #2-3 |
| 5 | **Enable Cloud SQL backups + deletion_protection** | 1 hour | None |

**Deferred to post-competition**: Full SQLite-to-PostgreSQL migration, ML-based instance classification, dashboard enhancements, alerting/cost tracking, Python-to-Rust codegen pipeline.

---

## Questions for Human Clarification

1. **Budget**: What is the single monthly cloud budget number? Is $150/month (including Cloud SQL) acceptable?
2. **Coverage target**: What percentage of anni2022 instances must be attempted before competition? All 5K, or a representative subset?
3. **Competition rules**: Have the 2026 Experimental Track rules been confirmed? Should we verify before proceeding?
4. **Phase 4 deferral**: Is a manually-constructed 3-5 entry parameter lookup table (based on ratio buckets) acceptable for the 2026 competition, with ML-based heuristics deferred?
5. **Concurrent workloads**: Should benchmark workers and Optuna workers ever run simultaneously, or is exclusive access to Cloud SQL acceptable?
6. **Family taxonomy**: Should we use GBD `family` field as the canonical family label, or define our own clustering?
7. **Sat2025 instances**: Include in the benchmark queue (with unknown difficulty), or defer until competition reference data exists?
