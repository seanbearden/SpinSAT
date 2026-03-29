# Design Document: Competition Benchmarking Pipeline

## Overview

This design covers the infrastructure for SpinSAT's SAT Competition 2026 preparation. It targets the **Experimental Track** (confirmed: no UNSAT certificates required, solver submission April 26).

**Budget**: $150/month ($25 Cloud SQL + $125 compute)
**Team**: One developer (Sean)
**Scope**: Continuous benchmarking + Optuna tuning + hardcoded parameter table

---

## Architecture

```
Sean's laptop                Cloud SQL (spinsat-optuna)         Spot VMs (MIG)
    |                             |                                 |
    |-- benchmark_queue.py ------>| spinsat_benchmarks DB           |
    |   populate / status / reap  |   work_queue table              |
    |                             |   benchmark_results             |<-- benchmark_worker.py
    |                             |   family_params                 |    (claim → solve → record)
    |                             |                                 |
    |-- optuna_tune.py ---------->| optuna DB                      |<-- optuna_tune.py
    |   --cloud                   |   (existing, unchanged)         |    (existing workers)
    |                             |                                 |
    |-- export-sqlite ----------->|                                 |
    |   benchmarks.db             |                                 |
    |   gh release upload ------->| GitHub Pages dashboard          |
```

---

## New Files

| File | Purpose |
|------|---------|
| `scripts/benchmark_queue.py` | CLI: populate queue, check status, reap zombies, export SQLite, show per-family stats |
| `scripts/benchmark_worker.py` | Long-running worker: claim → solve → record loop (runs on VMs). At claim time, if `solver_args` is NULL, worker looks up `family_params` for the instance's family and applies those parameters. This closes the loop: Optuna → family_params → next benchmark run. |
| `scripts/init_benchmarks_pg.py` | PostgreSQL schema DDL for `spinsat_benchmarks` database |
| `infra/terraform/bench-workers.tf` | MIG + instance template for benchmark workers |
| `infra/terraform/bench-worker-startup.sh.tpl` | VM startup script |

**Key data flow for `populate`**: The `benchmark_queue.py populate` command reads from **local** `competition_archive.db` (SQLite) and GBD instance metadata to set `priority` (competition best time) and `family`/`num_vars`/`num_clauses` columns. This avoids migrating 150K+ reference rows to Cloud SQL — the reference data stays local and is only used at queue creation time.

**No existing files are modified** except:
- `infra/terraform/cloudsql.tf` — enable backups, deletion_protection, add new database
- `src/main.rs` + `src/dmm.rs` — hardcoded parameter lookup table (Week 4)

---

## Data Model

### Work Queue (new `spinsat_benchmarks` database on existing Cloud SQL instance)

```sql
CREATE TABLE work_queue (
    id              BIGSERIAL PRIMARY KEY,
    campaign_id     TEXT NOT NULL,
    instance_hash   TEXT NOT NULL,
    instance_path   TEXT NOT NULL,       -- GCS path to CNF file
    family          TEXT,                -- GBD family (canonical)
    num_vars        INTEGER,
    num_clauses     INTEGER,
    priority        INTEGER NOT NULL DEFAULT 1000,  -- lower = higher priority
    timeout_s       INTEGER NOT NULL DEFAULT 60,
    solver_args     TEXT,

    status          TEXT NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending','running','done','failed','timeout')),
    worker_id       TEXT,
    started_at      TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ,
    heartbeat_at    TIMESTAMPTZ,
    enqueued_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    attempt         INTEGER NOT NULL DEFAULT 0,
    max_attempts    INTEGER NOT NULL DEFAULT 3,
    error_message   TEXT,
    result_status   TEXT,               -- SATISFIABLE/TIMEOUT/UNKNOWN
    result_time_s   DOUBLE PRECISION,
    result_json     JSONB,              -- full solver output metadata

    UNIQUE (campaign_id, instance_hash)
);

-- Worker claim: fast lock-free dequeue
CREATE INDEX idx_wq_claim ON work_queue (campaign_id, status, priority, enqueued_at)
    WHERE status = 'pending';

-- Reaper: find zombie rows
CREATE INDEX idx_wq_reaper ON work_queue (status, heartbeat_at)
    WHERE status = 'running';

-- Status aggregation
CREATE INDEX idx_wq_campaign_status ON work_queue (campaign_id, status);
```

### Campaigns Table

```sql
CREATE TABLE campaigns (
    campaign_id     TEXT PRIMARY KEY,
    description     TEXT,
    solver_version  TEXT NOT NULL,
    git_commit      TEXT,
    timeout_s       INTEGER NOT NULL,
    solver_args     TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at    TIMESTAMPTZ,
    status          TEXT NOT NULL DEFAULT 'active'
                    CHECK (status IN ('active','completed','cancelled'))
);
```

### Family Parameters Table

```sql
CREATE TABLE family_params (
    family          TEXT PRIMARY KEY,
    beta            DOUBLE PRECISION,
    gamma           DOUBLE PRECISION,
    delta           DOUBLE PRECISION,
    epsilon         DOUBLE PRECISION,
    alpha_initial   DOUBLE PRECISION,
    alpha_up_mult   DOUBLE PRECISION,
    alpha_down_mult DOUBLE PRECISION,
    alpha_interval  DOUBLE PRECISION,
    zeta            DOUBLE PRECISION,
    method          TEXT,
    restart_mode    TEXT,
    preprocess      BOOLEAN,
    source          TEXT,           -- 'optuna:study_name' or 'manual'
    par2_score      DOUBLE PRECISION,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

### Benchmark Results

For MVP, results are stored directly in `work_queue` columns (`result_status`, `result_time_s`, `result_json` JSONB). A separate `benchmark_results` table mirroring the SQLite schema is **post-competition** — avoids dual-write complexity.

### Instance Storage

Instances must be accessible from spot VMs. The existing GCS bucket `spinsat-benchmarks` already stores solver binaries and instances (used by `cloud_optuna.py`). The `populate` command uploads instances to GCS if not already present:

```bash
# Upload instances to GCS (one-time, ~5.3GB for sat2025, smaller for anni_2022)
gsutil -m cp benchmarks/competition/anni_2022/*.cnf gs://spinsat-benchmarks/instances/anni_2022/
```

The `instance_path` column stores the GCS path. Workers download instances via `gsutil cp` in the startup script (same pattern as `optuna-worker-startup.sh.tpl`).

### `solver_args` Format

The `solver_args` column stores space-separated CLI flags matching the solver's interface:

```
-m euler --beta 29.6 --gamma 0.43 --delta 0.064 --alpha-initial 11.2
```

The worker splits this and appends to the solver command. When NULL, the worker queries `family_params` and constructs args dynamically.

---

## Worker Design

### Claim Query (FOR UPDATE SKIP LOCKED)

```sql
UPDATE work_queue
SET status = 'running', worker_id = $1, started_at = NOW(),
    heartbeat_at = NOW(), attempt = attempt + 1
WHERE id = (
    SELECT id FROM work_queue
    WHERE campaign_id = $2 AND status = 'pending' AND attempt < max_attempts
    ORDER BY priority ASC, enqueued_at ASC
    LIMIT 1
    FOR UPDATE SKIP LOCKED
)
RETURNING *;
```

### Heartbeat + Reaper

- Workers update `heartbeat_at = NOW()` every **30 seconds** via background thread
- Reaper (manual or cron): resets rows where `status='running' AND heartbeat_at < NOW() - INTERVAL '3 minutes'` to `pending`
- Rows exceeding `max_attempts` (default 3) are set to `failed`
- Worker handles SIGTERM (spot preemption): releases current row back to `pending`
- **Family-tuned parameters**: If `solver_args` is NULL on the claimed row, worker queries `family_params` table for the instance's `family`. If found, applies those parameters (beta, gamma, etc.) to the solver invocation. This enables S2: "next benchmark run uses tuned parameters automatically" without re-populating the queue. The `populate` command also supports `--use-family-params` to pre-bake parameters into `solver_args` at enqueue time for explicit control.

### Connection Pooling

All workers enforce `pool_size=2, max_overflow=0`:

```python
engine = create_engine(db_url, pool_size=2, max_overflow=0,
                       pool_recycle=1800, pool_pre_ping=True)
```

**Connection budget** (100 max on db-g1-small):

| Consumer | Instances | Connections each | Peak |
|----------|-----------|-----------------|------|
| Benchmark workers | 4 | 2 | 8 |
| Optuna workers | 4 | 2 | 8 |
| Campaign CLI (Sean) | 1 | 2 | 2 |
| Local dev / dashboard | 1 | 2 | 2 |
| Reaper | 1 | 1 | 1 |
| **Total** | | | **21** |
| **Headroom** | | | **79** |

**Verdict**: db-g1-small is sufficient. No upgrade needed.

---

## Compute Budget Math

| Activity | VM-hours | Cost | Timeline |
|----------|----------|------|----------|
| 60s sweep (all 5K instances) | 21 | **$0.71** | Day 1 |
| Full 5000s runs (prioritized) | 500-1500 | $17-51 | Weeks 1-3 |
| Optuna tuning campaigns | 500-2000 | $17-68 | Ongoing |
| **Total compute** | | **$35-120** | |
| Cloud SQL (fixed) | | $25 | Monthly |
| **Grand total** | | **$60-145** | Within $150 |

Using c3-standard-4 spot VMs at ~$0.034/hr. Each VM runs 4 concurrent solves (one per core).

---

## Security (Priority Order)

| # | Change | Effort | File |
|---|--------|--------|------|
| 1 | `deletion_protection = true` | 1 line | `cloudsql.tf` |
| 2 | Enable backups (7-day, 4AM UTC) | 5 lines | `cloudsql.tf` |
| 3 | Restrict `authorized_networks` from `0.0.0.0/0` | 1 var change | `variables.tf` |
| 4 | Move DB password to Secret Manager | 1-2 hours | startup scripts |
| 5 | Create read-only `dashboard_reader` user | 10 min | `cloudsql.tf` |
| 6 | Dedicated service account for workers | 30 min | `bench-workers.tf` |
| 7 | Cloud SQL Auth Proxy for local dev | 15 min | local setup |

Items 1-3 are done in the first Terraform apply (Week 1). Items 4-7 follow in Week 1-2.

---

## Rust Solver Integration

Hardcoded parameter lookup in `src/dmm.rs` (next to existing `with_auto_zeta`):

```rust
pub fn select_competition_profile(ratio: f64, num_vars: usize) -> Option<Params> {
    // Family-aware tuning from Optuna campaigns
    // Generated by: benchmark_queue.py best-params --format rust
    //
    // Dispatch order: ratio bucket -> family-specific override -> defaults
    // The ratio thresholds correspond to structural regimes:
    //   ratio < 3.5: under-constrained (easy random)
    //   3.5-4.5: critical region (near alpha_r ≈ 4.27)
    //   4.5-5.5: over-constrained
    //   > 5.5: structured/crafted instances
    match () {
        _ if ratio < 3.5 => Some(Params { beta: 30.0, gamma: 0.15, .. }),
        _ if ratio < 4.5 => Some(Params { beta: 20.0, gamma: 0.25, .. }),
        _ if ratio < 5.5 => Some(Params { beta: 25.0, gamma: 0.20, .. }),
        _ => None, // use defaults for structured instances
    }
    // NOTE: Family-based dispatch (matching GBD family strings) is the
    // target for the ML heuristic bead. For the competition MVP, ratio-based
    // dispatch covers random instances well. Structured instances (ratio > 5.5)
    // may benefit from family-specific tuning in a future iteration.
}
```

Called from `main.rs` only when no CLI parameter overrides are given. The `benchmark_queue.py best-params --format rust` command generates this from Cloud SQL `family_params` data. For the MVP, it produces ratio-bucket dispatch. The ML heuristic bead (planned separately) will extend this to use family + structural features.

---

## CLI Workflow Summary

```bash
# === WEEK 1: Foundation ===
terraform apply                          # Enable backups, create new DB
python3 scripts/init_benchmarks_pg.py    # Create tables

# === WEEK 2: 60s Sweep ===
python3 scripts/benchmark_queue.py populate \
    --instances "benchmarks/competition/anni_2022/**/*.cnf" \
    --timeout 60 --campaign-id sweep-60s
terraform apply -var="bench_worker_count=4"   # Start workers
python3 scripts/benchmark_queue.py status     # Monitor progress
python3 scripts/benchmark_queue.py reap       # Fix stuck rows

# === WEEK 3: Analyze + Tune ===
python3 scripts/benchmark_queue.py families   # Per-family solve rates
python3 scripts/optuna_tune.py --campaign campaigns/tune_qhid.yaml --cloud
python3 scripts/benchmark_queue.py best-params --format rust

# === WEEK 4: Build + Submit ===
# Paste parameters into src/dmm.rs
cargo build --release --target x86_64-unknown-linux-musl
# Submit by April 26

# === Ongoing ===
python3 scripts/benchmark_queue.py export-sqlite --output benchmarks.db
gh release upload vX.Y.Z benchmarks.db --clobber   # Updates dashboard
```

---

## Sprint Plan

### Week 1 (Mar 30 - Apr 5): Foundation
- [ ] Terraform: enable backups, deletion_protection, new `spinsat_benchmarks` DB
- [ ] Terraform: restrict `authorized_networks` (use `35.192.0.0/12` for GCP + Sean's IP)
- [ ] `scripts/init_benchmarks_pg.py` — full schema DDL
- [ ] `scripts/benchmark_queue.py` — `populate`, `status`, and `reap` subcommands
- [ ] `scripts/benchmark_worker.py` — claim/heartbeat/result/SIGTERM loop
- [ ] `infra/terraform/bench-workers.tf` + `bench-worker-startup.sh.tpl`
- [ ] Upload anni_2022 instances to GCS: `gsutil -m cp ... gs://spinsat-benchmarks/instances/`
- [ ] `gcloud billing budgets create` — alerts at $120 and $150
- [ ] Test worker locally against Cloud SQL with 5 tiny CNF instances

### Week 2 (Apr 6 - 12): 60s Sweep
- [ ] Populate queue: all anni_2022 instances, 60s timeout, priority from `competition_archive.db`
- [ ] Deploy 4 spot VMs, run sweep
- [ ] Verify: no duplicate claims (check `worker_id` uniqueness per row)
- [ ] Test reaper: `kill -SIGTERM` a worker, verify row resets to pending
- [ ] Analyze results: classify instances by difficulty + family

### Week 3 (Apr 13 - 19): Tuning + Registration
- [ ] Run Optuna campaigns on weakest families (existing `--cloud` flow)
- [ ] `benchmark_queue.py families` and `best-params` subcommands
- [ ] Populate `family_params` from Optuna results
- [ ] Queue sat2025 instances (with available reference data if found)
- [ ] **April 19: Register solver**

### Week 4 (Apr 20 - 26): Submit
- [ ] Build hardcoded parameter table in `src/dmm.rs`
- [ ] **Validate**: Run 5000s campaign with tuned params vs default params on same 200-instance subset, compare PAR-2
- [ ] Re-run 5000s on promising instances with family-tuned params
- [ ] Rebuild musl binary with parameter table
- [ ] Export results, update dashboard
- [ ] **April 26: Submit solver**

### Fallback Plan (if Weeks 1-2 slip past April 12)
If the pipeline isn't ready by April 12:
- **Skip cloud benchmarking entirely** for competition submission
- Use existing `benchmark_suite.py --cloud` for ad-hoc GCP runs
- Run manual Optuna campaigns with existing `optuna_tune.py --cloud` (already works)
- Build parameter table from existing tuning data (barthel: PAR-2=13.22, qhid: PAR-2=26.31 from handoff)
- Submit solver with best-known parameters from prior Optuna runs
- Build pipeline post-competition for 2027

### Budget Monitoring (set up in Week 1)
- [ ] `gcloud billing budgets create` — alert at 80% ($120) and 100% ($150)
- [ ] Alerts go to Sean's email
- [ ] Manual scale-to-zero: `terraform apply -var="bench_worker_count=0"`

### Critical Path (Week 1 priorities)
The following are blocking for Week 2's 60s sweep. If Week 1 slips, these must ship first:
1. **P0**: `init_benchmarks_pg.py` (schema DDL) — everything depends on this
2. **P0**: `benchmark_worker.py` (claim/solve/record loop) — the core deliverable
3. **P0**: Terraform: new `spinsat_benchmarks` DB + backups + deletion_protection
4. **P1**: `benchmark_queue.py populate` + `status` — needed to load and monitor the sweep
5. **P2**: `bench-workers.tf` + startup script — can test workers locally first, deploy to cloud in Week 2
6. **P3**: Security items 3-7 (network restriction, Secret Manager, etc.) — can follow in Week 2

---

## Open Decisions (Resolved)

| Question | Decision | Rationale |
|----------|----------|-----------|
| Cloud SQL tier | Keep db-g1-small | 21 peak connections, well within 100 limit |
| Work queue mechanism | PostgreSQL FOR UPDATE SKIP LOCKED | Simple, no new infra |
| Dashboard hosting | Keep GitHub Pages + SQLite export | No backend changes for MVP |
| Parameter table format | Rust `match` statement, manual edit | Zero infra, testable, backward-compatible |
| Family taxonomy | GBD `family` field | Human decision, avoids clustering complexity |
| Backup strategy | Automated daily + deletion_protection | ~$1/month, prevents catastrophic loss |
| Budget control | Manual `worker_count=0` via Terraform | Simple, no auto-scaling complexity |

---

## Reference: SAT Competition 2026

- **Tracks**: Main, Experimental, Parallel, Cloud, AI-Generated/AI-Tuned sub-tracks
- **Experimental Track**: No certificate requirement. Must outperform top 3 Main Track for award.
- **Hardware**: HoreKa Blue Instance, Intel Xeon Platinum 8368, 8 benchmarks in parallel
- **Limits**: 5000s timeout, 32GB memory
- **Deadlines**: Registration April 19, Submission April 26, Documentation May 17
- **Requirements**: Source code (research license), 1-2 page system description (IEEE format)
- **SAT 2025 had NO Experimental Track** (only Main + No Limits + Parallel)
- **SAT 2025 proceedings**: `docs/sat_competition_2025_proceedings.pdf` (solver technique descriptions)

Source: [SAT Competition 2026](https://satcompetition.github.io/2026/), [Tracks](https://satcompetition.github.io/2026/tracks.html), [Rules](https://satcompetition.github.io/2026/rules.html)
