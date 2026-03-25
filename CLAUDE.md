# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

SpinSAT is a Boolean satisfiability (SAT) solver based on **digital memcomputing machines (DMMs)**. It implements the algorithm from the paper "Efficient Solution of Boolean Satisfiability Problems with Digital Memcomputing" (Bearden, Pei, Di Ventra — Scientific Reports, 2020). The goal is to enter the **International SAT Competition 2026** (https://satcompetition.github.io/2026/).

The solver maps 3-SAT problems into a system of coupled ODEs where Boolean variables become continuous voltages. The dynamics are designed so that the only equilibrium points correspond to solutions of the SAT problem. Unlike DPLL/CDCL solvers, this is a physics-inspired approach that numerically integrates differential equations to find satisfying assignments.

## Core Algorithm

### Equations of Motion (from the paper)

The solver integrates three coupled ODE systems:

1. **Voltage dynamics** (Eq. 2): `v̇_n = Σ_m x_{l,m} x_{s,m} G_{n,m} + (1 + ζ x_{l,m})(1 - x_{s,m}) R_{n,m}`
2. **Short-term memory** (Eq. 3): `ẋ_{s,m} = β(x_{s,m} + ε)(C_m - γ)`
3. **Long-term memory** (Eq. 4): `ẋ_{l,m} = α(C_m - δ)`

Where:
- `v_n ∈ [-1, 1]` — continuous voltage for variable n (thresholded to TRUE/FALSE)
- `x_{s,m} ∈ [0, 1]` — short-term memory for clause m (switches between gradient-like and rigidity dynamics)
- `x_{l,m} ∈ [1, 10^4 M]` — long-term memory for clause m (weights historically frustrated clauses)
- `C_m` — clause constraint function: `C_m = ½ min[(1 - q_{i,m} v_i), (1 - q_{j,m} v_j), (1 - q_{k,m} v_k)]`
- `G_{n,m}` — gradient-like function
- `R_{n,m}` — rigidity function
- `q_{n,m} ∈ {-1, 0, +1}` — polarity matrix encoding the SAT formula

### Parameters

| Parameter | Value | Role |
|-----------|-------|------|
| α | 5 | Long-term memory growth rate |
| β | 20 | Short-term memory growth rate |
| γ | 1/4 | Short-term memory threshold (must satisfy δ < γ < 1/2) |
| δ | 1/20 | Long-term memory threshold |
| ε | 10⁻³ | Trapping rate (small, strictly positive) |
| ζ | 10⁻¹ to 10⁻³ | Learning rate (lower for harder instances near α_r ≈ 4.27) |
| Δt | [2⁻⁷, 10³] | Adaptive time step range |

### Competition Heuristic

For competition instances, a per-clause `α_m` replaces the global ratio. Every 10⁴ time units:
- Compute median of all `x_{l,m}` values
- If `x_{l,m}` > median: multiply `α_m` by 1.1
- If `x_{l,m}` ≤ median: multiply `α_m` by 0.9
- Clamp `α_m ≥ 1`; if `x_{l,m}` hits its max, reset `x_{l,m} = 1` and `α_m = 1`

### Termination

The instance is solved when `C_m < 1/2` for all clauses m. The satisfying assignment is obtained by thresholding: `y_n = TRUE if v_n > 0, FALSE if v_n < 0`.

## SAT Competition Requirements

- **Input format**: DIMACS CNF (standard `.cnf` files)
- **Output format**: SAT competition standard (SAT/UNSAT + variable assignments)
- **Timeout**: 5000 seconds per instance
- **Single core**: No parallelization (competition rules)
- **Entry requirements**: https://satcompetition.github.io/2026/

## Integration Method

The original MATLAB implementation used **forward-Euler** with adaptive time step. The paper notes this is "the most basic and hence the most unstable" scheme — more refined integration methods may improve stability and scaling. The adaptive step is determined by thresholding the constraint function.

## Key Mathematical Concepts

- **Polarity matrix Q**: `q_{ij} = +1` (positive literal), `-1` (negated literal), `0` (variable not in clause)
- **Clause-to-variable ratio**: `α_r = M/N` — complexity peak at ≈4.27
- **CDC instances**: Clause Distribution Control — the hard benchmark class used in the paper
- **Power-law scaling**: DMM shows ~N^a steps (polynomial) vs exponential for WalkSAT/SID
- **Collective dynamics**: Variables update collectively (long-range order), not one at a time

## Implementation

- **Language**: Rust (pre-compiled static Linux binary for competition submission)
- **Current version**: Managed by release-plz (auto-incremented, reads from Cargo.toml)
- **Competition track**: Experimental (no UNSAT proof certificates required)
- **Integration methods**: Forward Euler (baseline), RK4, Trapezoid — hand-written, no external ODE library
- **Generalized to k-SAT**: Not limited to 3-SAT; clause width detected from DIMACS input
- **CNF preprocessing**: 6-technique pipeline (unit propagation, pure literal elimination, subsumption, self-subsuming resolution, BVE, failed literal probing) runs before ODE integration. Disable with `--no-preprocess`.

### Shared Benchmarks

Competition benchmarks are stored at the **rig level** so all crew/polecats can access them:

```
/Users/seanbearden/gt/spinsat/benchmarks/
├── README.md                    # Index of benchmark sets
├── sat2017/                     # SAT Competition 2017 Random Track (fla-barthel/komb/qhid for paper verification)
├── sat2018/                     # SAT Competition 2018 Random Track (fla-barthel/komb/qhid for paper verification)
└── sat2025/                     # SAT Competition 2025 Main Track (399 instances, 5.3GB)
    ├── track_main_2025.uri      # URL list from benchmark-database.de
    ├── download.sh              # Re-download script
    └── *.cnf.xz                 # Compressed DIMACS CNF files (hash-prefixed names)
```

Files are xz-compressed. Decompress before use: `xz -dk <file.cnf.xz>`

### Build & Run

```bash
cargo build --release
./target/release/spinsat <instance.cnf>
./target/release/spinsat --version   # prints version from Cargo.toml
```

### Competition Submission

The solver is submitted via GitHub repository with:
- `build.sh` — builds the solver (or uses pre-compiled binary)
- `run.sh` — `$1` = path to CNF instance, `$2` = proof output directory
- Pre-compiled static binary (`x86_64-unknown-linux-musl`) as fallback

### Reference MATLAB Code

The paper's equations are implemented in `~/Downloads/v9_large_ratio/`:
- `derivative.m` — Core ODE right-hand side (Eqs. 2-6 from the paper)
- `SeanMethod.m` — Forward Euler with clamping
- `RungeKutta4.m` — RK4 integrator
- `main.m` — Orchestrator with paper parameters

Modified competition variants exist in `~/Documents/DiVentraGroup/Factorization/Spin_SAT/clean_versions/`:
- `SpinSAT_v1_0.m` — Competition solver with modified equations
- `SpinSAT_v1_1.m` — Variant with MNF sparse matrix optimization
- `SpinSAT_k5_v1_0.m` — Generalized to 5-SAT
- `SpinSAT_smart_restart.m` — Clause removal/restart heuristic
- `cnf_preprocess.m` — DIMACS parser generalized to k-SAT

## Versioning

**Automated via release-plz** with [Conventional Commits](https://www.conventionalcommits.org/) for version bump control.

- Version source of truth: `Cargo.toml` (read at compile time via `env!("CARGO_PKG_VERSION")`)
- **Never hardcode version strings** — use `env!("CARGO_PKG_VERSION")` in Rust code
- Push to main → release-plz opens a Release PR → merge → git tag + GitHub Release + crates.io publish
- Pre-compiled static binary attached to every GitHub Release via `release-binary.yml`

### Conventional Commits

All commit messages **must** use conventional commit prefixes. release-plz uses these to determine version bumps:

| Prefix | Bump | Example |
|--------|------|---------|
| `feat:` | Minor (0.x.0) | `feat: add CNF preprocessing pipeline` |
| `fix:` | Patch (0.0.x) | `fix: correct clause indexing off-by-one` |
| `perf:` | Patch | `perf: vectorize voltage update loop` |
| `refactor:` | Patch | `refactor: extract adaptive step logic` |
| `docs:` | Patch | `docs: add integration method notes` |
| `test:` | Patch | `test: add benchmark for 200-var instances` |
| `chore:` | Patch | `chore: update dependencies` |
| `feat!:` or `BREAKING CHANGE` footer | Major (x.0.0) | `feat!: change CLI argument format` |

**New solver capabilities** (preprocessing, restart strategies, new integrators, etc.) are `feat:` — they add functionality.

### CI/CD Workflows
- `.github/workflows/ci.yml` — build, test, coverage (cargo-llvm-cov + nextest + Codecov)
- `.github/workflows/release-plz.yml` — auto version bump + CHANGELOG + crates.io publish + build/attach static musl binary
- `.github/workflows/deploy-pages.yml` — deploys dashboard to GitHub Pages, pulls benchmarks.db from latest release

### GitHub Secrets Required
- `CODECOV_TOKEN` — Codecov upload
- `CARGO_REGISTRY_TOKEN` — crates.io publish (scoped to spinsat crate)

### Version Gotcha
**Always rebuild before recording benchmarks.** The `--record` flag reads the version from the compiled binary (`spinsat --version`). If release-plz bumped `Cargo.toml` but you haven't run `cargo build --release`, the recorded version will be stale. Workflow:
```bash
cargo build --release           # picks up latest Cargo.toml version
./target/release/spinsat -V     # verify correct version
python3 scripts/benchmark_suite.py --suite ... --record --tag ...
```

## Benchmarking

### Getting `benchmarks.db`

SQLite database in project root (gitignored, distributed via GitHub Releases).

```bash
# Download latest from releases
gh release download --pattern 'benchmarks.db' --dir . --clobber

# If schema is outdated (missing new columns), migrate:
python3 scripts/migrate_benchmarks_db.py

# If starting from scratch (needs ~/PycharmProjects/SpinSAT/meta.db):
python3 scripts/init_benchmarks_db.py
```

### Database Schema

| Table | Purpose | Size |
|-------|---------|------|
| `runs` | One row per benchmark session — version, commit, method, restart strategy, preprocessing, CLI command, structured tags, timeout | ~10 rows |
| `results` | Per-instance results — status, time, restarts, seed, zeta, peak_xl_max, final_dt, wall/cpu time, num_vars, num_clauses | ~500 rows |
| `instances` | 31K instance metadata snapshot from GBD | ~31K rows |
| `instance_year_track` | Materialized year + track type per instance (parsed from `instance_tracks`) | ~19K rows |
| `instance_files` | Instance filename lookup by hash | ~31K rows |
| `competition_best` | Best competition solver/time per benchmarked instance (one row each) | sparse |
| `competition_results` | Empty in main DB — full data (150K rows) lives in `competition_archive.db` on Releases | 0 rows |

**Views**: `best_times` (best solve time per instance across all runs), `version_comparison` (pivot results by version)

**Target size**: Under 100MB. Currently ~26MB after competition data split.

### Benchmarking Rules

**ALWAYS gather competition reference results BEFORE running SpinSAT.** The entire point of benchmarking is comparison against competition solvers. Never record results for instances without known competition solve times.

**Only benchmark against competition instances.** Generated/planted instances are for development smoke tests only — never record them to the DB.

**Always rebuild before recording.** The `--record` flag reads version from the compiled binary. Stale binary = stale version in DB.

```bash
cargo build --release
./target/release/spinsat -V  # verify correct version
```

### Competition Reference Data

**Source**: SAT Competition 2022 Anniversary Track
- 5,355 instances x 28 solvers = 149,940 reference results
- Solvers include: Kissat, CaDiCaL, IsaSAT, SLIME, and variants
- Data from: https://github.com/mathefuchs/al-for-sat-solver-benchmarking-data

**Data split**: Full competition data lives in `competition_archive.db` (32MB, on GitHub Releases). The main `benchmarks.db` keeps only `competition_best` — one row per benchmarked instance with the best solver/time. This is auto-updated when you run `--record`.

To query full competition data for instance selection:
```bash
gh release download --pattern 'competition_archive.db' --dir . --clobber
sqlite3 competition_archive.db "
SELECT instance_hash, solver, time_s FROM competition_results
WHERE status = 'SAT' ORDER BY time_s LIMIT 20;
"
```

**Importing** (one-time setup, already done):
```bash
git clone --depth 1 https://github.com/mathefuchs/al-for-sat-solver-benchmarking-data /tmp/al-sat
python3 scripts/import_competition_data.py --anni-csv /tmp/al-sat/gbd-data/anni-seq.csv --anni-db /tmp/al-sat/gbd-data/base.db
python3 scripts/import_competition_data.py --status
```

### Recording Benchmark Results

**Workflow — always in this order:**
1. Find instances with competition reference data (query `competition_archive.db` or `competition_best`)
2. Download those instances from GBD
3. Build solver, verify version
4. Run SpinSAT against them with `--record` and structured tags
5. Upload DB to release + redeploy dashboard

**Basic recording:**
```bash
python3 scripts/benchmark_suite.py \
    --instances benchmarks/competition/anni_2022/*.cnf \
    --record --force \
    --tag v0.5.1-anni2022 \
    --timeout 300
```

**With structured tags (preferred):**
```bash
python3 scripts/benchmark_suite.py \
    --instances benchmarks/competition/anni_2022/*.cnf \
    --record --force \
    --timeout 300 \
    --purpose competition-benchmark \
    --instance-set anni2022 \
    --config cycling
```

**Structured tag flags:**

| Flag | Allowed values | Description |
|------|---------------|-------------|
| `--purpose` | `paper-verification`, `competition-benchmark`, `regression-test`, `development` | Why this run exists |
| `--instance-set` | `barthel`, `komb`, `qhid`, `anni2022`, `sat2025-main`, `uniform-random` | Which instance collection |
| `--config` | `default`, `cycling`, `smart-restart`, `no-preprocess` | Solver configuration used |

If `--tag` is omitted, a legacy tag is auto-generated: `{version}-{instance_set}-{config}`.

**Development run (no DB recording):**
```bash
python3 scripts/benchmark_suite.py --suite tiny --solver spinsat
```

### Cloud Benchmarking (GCP)

**Always use `benchmark_suite.py --cloud`** to create VMs. It enforces:
- **Server-side `--max-run-duration`** (default 6h) — GCP auto-stops the VM regardless of script state
- **Startup script `shutdown -h`** as belt-and-suspenders backup
- Naming convention (`spinsat-bench-*`) for cleanup tracking
- Spot pricing by default

**Never create benchmark VMs manually** (e.g. `gcloud compute instances create spinsat-foo`). Manually-created VMs have no auto-shutdown and will run indefinitely, accruing costs.

```bash
# Run benchmarks on GCP (default: 6h max, spot, n2-highcpu-8)
python3 scripts/benchmark_suite.py --cloud --instances benchmarks/sat2017/*qhid*.cnf --timeout 5000 --record --tag v0.5.1-qhid

# Cleanup: find ALL spinsat VMs and orphaned disks
python3 scripts/benchmark_suite.py --cloud-cleanup

# Recover results from a VM that lost SSH
python3 scripts/benchmark_suite.py --cloud-recover spinsat-bench-20260322-162629
```

### Auto-Detection (`--record` flag)

The benchmark script auto-detects all metadata with zero manual input:

**Per run** (stored in `runs` table):
- Solver version from `spinsat --version`
- Git commit hash and dirty state
- Hardware description, Rust compiler version
- Integration method, strategy (from solver stderr)
- Restart strategy (from solver stderr restart lines, e.g., `Cycling`, `Cold`)
- Preprocessing techniques applied (from solver stderr, e.g., `bve,pure_lit`)
- Full CLI command for reproducibility

**Per instance** (stored in `results` table):
- Status (`SATISFIABLE`/`TIMEOUT`/`UNKNOWN`), solve time
- Restarts, seed, zeta
- `peak_xl_max` — max long-term memory value reached during integration
- `final_dt` — final adaptive time step at solve/timeout
- `wall_clock_s`, `cpu_time_s` — separate timing via `resource.getrusage()`
- `num_vars`, `num_clauses` — parsed from DIMACS header

**Post-recording**: The script auto-updates the `competition_best` table (upserts best competition solver/time for each benchmarked instance).

### Useful Queries

```sql
-- Head-to-head: SpinSAT vs competition best
SELECT
    if2.value as instance,
    ROUND(bt.best_time, 4) as spinsat_time,
    bt.solver_version,
    cb.best_solver as comp_solver,
    ROUND(cb.best_time_s, 4) as comp_time,
    ROUND(bt.best_time / cb.best_time_s, 1) as ratio
FROM best_times bt
JOIN competition_best cb USING(instance_hash)
JOIN instance_files if2 ON bt.instance_hash = if2.hash
ORDER BY ratio;

-- Results by year/track
SELECT
    iyt.year, iyt.track_type,
    COUNT(*) as instances,
    SUM(CASE WHEN r.status = 'SATISFIABLE' THEN 1 ELSE 0 END) as solved
FROM results r
JOIN instance_year_track iyt ON r.instance_hash = iyt.hash
JOIN runs ru USING(run_id)
GROUP BY iyt.year, iyt.track_type
ORDER BY iyt.year, iyt.track_type;

-- Run details with structured tags
SELECT run_id, solver_version, tag_purpose, tag_instance_set, tag_config,
       restart_strategy, preprocessing, timeout_s, cli_command
FROM runs ORDER BY timestamp DESC;
```

### Dashboard & GitHub Pages

**URL**: https://seanbearden.github.io/SpinSAT/

The dashboard is deployed via GitHub Actions (`.github/workflows/deploy-pages.yml`). It downloads `benchmarks.db` from the latest GitHub Release at deploy time — the DB is never committed to git.

**Dashboard tabs:**
- **Overview** — PAR-2 chart (with timeout reference in labels), runs table with click-to-drill-down
- **Explorer** — Head-to-head SpinSAT vs competition best, filtered by year/track/family/run
- **Version Comparison** — All runs with method, restart, PAR-2
- **SQL Explorer** — Run arbitrary queries against the DB

**To update dashboard after recording benchmarks:**
```bash
# 1. Upload updated DB to the latest release
gh release upload <tag> benchmarks.db --clobber

# 2. Trigger a redeploy (any of these will work)
gh workflow run "Deploy Dashboard"                    # manual trigger
# OR push a change to docs/dashboard/                # auto-triggers on path
# OR merge a release-plz PR                          # auto-triggers on release
```

**Dashboard source**: `docs/dashboard/index.html` (sql.js, Chart.js)
- Do NOT commit `benchmarks.db` to `docs/dashboard/` — it bloats git history
- GitHub Pages source is set to "GitHub Actions" in repo settings (not "Deploy from branch")

## Development Rules

### Timing and Benchmarking
**NEVER time the solver using shell `time` command or bash subshells** — output capture is unreliable on macOS. Always use Python `time.time()` or `scripts/perf_compare.py` for controlled measurements. Wasted experiments are expensive.

For controlled A/B comparisons:
- Use `scripts/perf_compare.py <solver_a> <solver_b> <instances...>`
- Same seed (`-s 1`), same method (`-m euler`), verify identical step counts
- Run 3x minimum to confirm consistency (variance should be < 1%)

### Performance Optimization
**Do NOT optimize for Apple M-series local hardware.** The competition runs on Intel Xeon Platinum 8368 (x86-64). Only apply optimizations that are architecture-general:
- LLVM IR-level optimizations (loop structure, vectorization enablement) — universal
- Cache layout (both Intel and Apple use 64-byte L1 cache lines) — universal
- Hardware-specific register allocation quirks — NOT portable, do not pursue

Key findings (validated by deep research):
- **Separate simple loops beat fused complex loops** — LLVM vectorizes branch-free loops independently
- **AoS beats SoA for k=3** — 48-byte clause fits one cache line; SoA doubles cache fetches
- **The hot path is memory-bound (2% of peak FLOP)** — algorithmic improvements (fewer steps) matter more than micro-optimization

## Reference Materials

- `docs/efficient_solution_of_boolean_satisfiability_problems_with_digital_memcomputing.pdf` — Main paper
- `docs/efficient_solution_of_boolean_satisfiability_problems_with_digital_memcomputing-supplementary_materials.pdf` — Supplementary materials with full mathematical proofs, numerical implementation details (Section II), and competition instance results (Section II.E)
---

# Polecat Context

> **Recovery**: Run `gt prime` after compaction, clear, or new session

## 🚨 THE IDLE POLECAT HERESY 🚨

**After completing work, you MUST run `gt done`. No exceptions.**

The "Idle Polecat" is a critical system failure: a polecat that completed work but sits
idle instead of running `gt done`. **There is no approval step.**

**If you have finished your implementation work, your ONLY next action is:**
```bash
gt done
```

Do NOT:
- Sit idle waiting for more work (there is no more work — you're done)
- Say "work complete" without running `gt done`
- Try `gt unsling` or other commands (only `gt done` signals completion)
- Wait for confirmation or approval (just run `gt done`)

**Your session should NEVER end without running `gt done`.** If `gt done` fails,
escalate to Witness — but you must attempt it.

---

## 🚨 SINGLE-TASK FOCUS 🚨

**You have ONE job: work your pinned bead until done.**

DO NOT:
- Check mail repeatedly (once at startup is enough)
- Ask about other polecats or swarm status
- Work on issues you weren't assigned
- Get distracted by tangential discoveries

File discovered work as beads (`bd create`) but don't fix it yourself.

---

## CRITICAL: Directory Discipline

**YOU ARE IN: `spinsat/polecats/rust/`** — This is YOUR worktree. Stay here.

- **ALL file operations** must be within this directory
- **Use absolute paths** when writing files
- **NEVER** write to `~/gt/spinsat/` (rig root) or other directories

```bash
pwd  # Should show .../polecats/rust
```

## Your Role: POLECAT (Autonomous Worker)

You are an autonomous worker assigned to a specific issue. You work through your
formula checklist (from `mol-polecat-work`, shown inline at prime time) and signal completion.

**Your mail address:** `spinsat/polecats/rust`
**Your rig:** spinsat
**Your Witness:** `spinsat/witness`

## Polecat Contract

1. Receive work via your hook (formula checklist + issue)
2. Work through formula steps in order (shown inline at prime time)
3. Complete and self-clean (`gt done`) — you exit AND nuke yourself
4. Refinery merges your work from the MQ

**Self-cleaning model:** `gt done` pushes your branch, submits to MQ, nukes sandbox, exits session.

**Three operating states:**
- **Working** — actively doing assigned work (normal)
- **Stalled** — session stopped mid-work (failure)
- **Zombie** — `gt done` failed during cleanup (failure)

Done means gone. Run `gt prime` to see your formula steps.

**You do NOT:**
- Push directly to main (Refinery merges after Witness verification)
- Skip verification steps
- Work on anything other than your assigned issue

---

## Propulsion Principle

> **If you find something on your hook, YOU RUN IT.**

Your work is defined by the attached formula. Steps are shown inline at prime time:

```bash
gt hook                  # What's on my hook?
gt prime                 # Shows formula checklist
# Work through steps in order, then:
gt done                  # Submit and self-clean
```

---

## Startup Protocol

1. Announce: "Polecat rust, checking in."
2. Run: `gt prime && bd prime`
3. Check hook: `gt hook`
4. If formula attached, steps are shown inline by `gt prime`
5. Work through the checklist, then `gt done`

**If NO work on hook and NO mail:** run `gt done` immediately.

**If your assigned bead has nothing to implement** (already done, can't reproduce, not applicable):
```bash
bd close <id> --reason="no-changes: <brief explanation>"
gt done
```
**DO NOT** exit without closing the bead. Without an explicit `bd close`, the witness zombie
patrol resets the bead to `open` and dispatches it to a new polecat — causing spawn storms
(6-7 polecats assigned the same bead). Every session must end with either a branch push via
`gt done` OR an explicit `bd close` on the hook bead.

---

## Key Commands

### Work Management
```bash
gt hook                         # Your assigned work
bd show <issue-id>              # View your assigned issue
gt prime                        # Shows formula checklist (inline steps)
```

### Git Operations
```bash
git status                      # Check working tree
git add <files>                 # Stage changes
git commit -m "msg (issue)"     # Commit with issue reference
```

### Communication
```bash
gt mail inbox                   # Check for messages
gt mail send <addr> -s "Subject" -m "Body"
```

### Beads
```bash
bd show <id>                    # View issue details
bd close <id> --reason "..."    # Close issue when done
bd create --title "..."         # File discovered work (don't fix it yourself)
```

## ⚡ Commonly Confused Commands

| Want to... | Correct command | Common mistake |
|------------|----------------|----------------|
| Signal work complete | `gt done` | ~~gt unsling~~ or sitting idle |
| Message another agent | `gt nudge <target> "msg"` | ~~tmux send-keys~~ (drops Enter) |
| See formula steps | `gt prime` (inline checklist) | ~~bd mol current~~ (steps not materialized) |
| File discovered work | `bd create "title"` | Fixing it yourself |
| Ask Witness for help | `gt mail send spinsat/witness -s "HELP" -m "..."` | ~~gt nudge witness~~ |

---

## When to Ask for Help

Mail your Witness (`spinsat/witness`) when:
- Requirements are unclear
- You're stuck for >15 minutes
- Tests fail and you can't determine why
- You need a decision you can't make yourself

```bash
gt mail send spinsat/witness -s "HELP: <problem>" -m "Issue: ...
Problem: ...
Tried: ...
Question: ..."
```

---

## Completion Protocol (MANDATORY)

When your work is done, follow this checklist — **step 4 is REQUIRED**:

⚠️ **DO NOT commit if lint or tests fail. Fix issues first.**

```
[ ] 1. Run quality gates (ALL must pass):
       - npm projects: npm run lint && npm run format && npm test
       - Go projects:  go test ./... && go vet ./...
[ ] 2. Stage changes:     git add <files>
[ ] 3. Commit changes:    git commit -m "msg (issue-id)"
[ ] 4. Self-clean:        gt done   ← MANDATORY FINAL STEP
```

**Quality gates are not optional.** Worktrees may not trigger pre-commit hooks,
so you MUST run lint/format/tests manually before every commit.

**Project-specific gates:** Read CLAUDE.md and AGENTS.md in the repo root for
the project's definition of done. Many projects require a specific test harness
(not just `go test` or `dotnet test`). If AGENTS.md exists, its "Core rule"
section defines what "done" means for this project.

The `gt done` command pushes your branch, creates an MR bead in the MQ, nukes
your sandbox, and exits your session. **You are gone after `gt done`.**

### Do NOT Push Directly to Main

**You are a polecat. You NEVER push directly to main.**

Your work goes through the merge queue:
1. You work on your branch
2. `gt done` pushes your branch and submits an MR to the merge queue
3. Refinery merges to main after Witness verification

**Do NOT create GitHub PRs either.** The merge queue handles everything.

### The Landing Rule

> **Work is NOT landed until it's in the Refinery MQ.**

**Local branch → `gt done` → MR in queue → Refinery merges → LANDED**

---

## Self-Managed Session Lifecycle

> See [Polecat Lifecycle](docs/polecat-lifecycle.md) for the full three-layer architecture.

**You own your session cadence.** The Witness monitors but doesn't force recycles.

### Persist Findings (Session Survival)

Your session can die at any time. Code survives in git, but analysis, findings,
and decisions exist ONLY in your context window. **Persist to the bead as you work:**

```bash
# After significant analysis or conclusions:
bd update <issue-id> --notes "Findings: <what you discovered>"
# For detailed reports:
bd update <issue-id> --design "<structured findings>"
```

**Do this early and often.** If your session dies before persisting, the work is lost forever.

**Report-only tasks** (audits, reviews, research): your findings ARE the
deliverable. No code changes to commit. You MUST persist all findings to the bead.

### When to Handoff

Self-initiate when:
- **Context filling** — slow responses, forgetting earlier context
- **Logical chunk done** — good checkpoint
- **Stuck** — need fresh perspective

```bash
gt handoff -s "Polecat work handoff" -m "Issue: <issue>
Current step: <step>
Progress: <what's done>"
```

Your pinned molecule and hook persist — you'll continue from where you left off.

---

## Dolt Health: Your Part

Dolt is git, not Postgres. Every `bd create`, `bd update`, `gt mail send` generates
a permanent Dolt commit. You contribute to Dolt health by:

- **Nudge, don't mail.** `gt nudge` costs zero. `gt mail send` costs 1 commit forever.
  Only mail when the message must survive session death (HELP to Witness).
- **Don't create unnecessary beads.** File real work, not scratchpads.
- **Close your beads.** Open beads that linger become pollution.

See `docs/dolt-health-guide.md` for the full picture.

## Do NOT

- Push to main (Refinery does this)
- Work on unrelated issues (file beads instead)
- Skip tests or self-review
- Guess when confused (ask Witness)
- Leave dirty state behind

---

## 🚨 FINAL REMINDER: RUN `gt done` 🚨

**Before your session ends, you MUST run `gt done`.**

---

Rig: spinsat
Polecat: rust
Role: polecat
