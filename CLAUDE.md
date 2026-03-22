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

### Database (`benchmarks.db`)

SQLite database in project root (gitignored, distributed via GitHub Releases). Contains:
- **Instance metadata**: 31,809 instances from SAT competition history (snapshot from `~/PycharmProjects/SpinSAT/meta.db`)
- **Benchmark results**: per-instance solve times with full reproducibility metadata
- **Competition reference**: SAT competition solve times for comparison
- **Views**: `best_times`, `version_comparison`

### Benchmarking Rules

**ALWAYS gather competition reference results BEFORE running SpinSAT.** The entire point of benchmarking is comparison against competition solvers. Never record results for instances without known competition solve times.

**Only benchmark against competition instances.** Generated/planted instances are for development smoke tests only — never record them to the DB.

**Workflow — always in this order:**
1. Query `competition_results` to find instances with known solve times
2. Download those instances from GBD
3. Run SpinSAT against them with `--record`
4. Upload DB to release + redeploy dashboard

### Competition Reference Data

**Source**: SAT Competition 2022 Anniversary Track (imported into `competition_results` table)
- 5,355 instances x 28 solvers = 149,940 reference results
- Solvers include: Kissat, CaDiCaL, IsaSAT, SLIME, and variants
- Data from: https://github.com/mathefuchs/al-for-sat-solver-benchmarking-data

**Importing** (one-time setup, already done):
```bash
git clone --depth 1 https://github.com/mathefuchs/al-for-sat-solver-benchmarking-data /tmp/al-sat
python3 scripts/import_competition_data.py --anni-csv /tmp/al-sat/gbd-data/anni-seq.csv --anni-db /tmp/al-sat/gbd-data/base.db
python3 scripts/import_competition_data.py --status
```

### Setup & Usage
```bash
# Initialize DB (one-time, snapshots meta.db)
python3 scripts/init_benchmarks_db.py

# Step 1: Find instances WITH competition reference data
sqlite3 benchmarks.db "
SELECT cr.instance_hash, if2.value, i.family, ROUND(MIN(cr.time_s), 2) as best_time
FROM competition_results cr
JOIN instances i ON cr.instance_hash = i.hash
JOIN instance_files if2 ON i.hash = if2.hash
WHERE cr.status = 'SAT'
GROUP BY cr.instance_hash
HAVING best_time BETWEEN 0.1 AND 60
ORDER BY best_time LIMIT 30;
"

# Step 2: Download those specific instances
python3 scripts/download_competition_instances.py --hashes <hash1> <hash2> ... --output-dir benchmarks/competition/anni_2022

# Step 3: Run SpinSAT and record
python3 scripts/benchmark_suite.py --instances benchmarks/competition/anni_2022/*.cnf --record --force --tag v0.4.1-anni2022 --timeout 300

# Step 4: Upload and redeploy dashboard
gh release upload <tag> benchmarks.db --clobber
gh workflow run "Deploy Dashboard"

# Development run (JSON only, no DB recording)
python3 scripts/benchmark_suite.py --suite tiny --solver spinsat

# Refresh instance metadata from meta.db
python3 scripts/init_benchmarks_db.py --refresh
```

### Auto-Detection (`--record` flag)
The benchmark script auto-detects with zero manual input:
- Solver version from `spinsat --version`
- Git commit hash and dirty state
- Hardware description
- Rust compiler version
- ODE parameters from SpinSAT stderr (strategy, zeta, seed, restarts, method)

### Dashboard & GitHub Pages

**URL**: https://seanbearden.github.io/SpinSAT/

The dashboard is deployed via GitHub Actions (`.github/workflows/deploy-pages.yml`), NOT from the `docs/` branch directly. The workflow downloads `benchmarks.db` from the latest GitHub Release at deploy time, so the DB is never committed to git.

**To update dashboard data after recording new benchmarks:**
```bash
# 1. Upload updated DB to the latest release
gh release upload <tag> benchmarks.db --clobber

# 2. Trigger a redeploy (any of these will work)
gh workflow run "Deploy Dashboard"                    # manual trigger
# OR push a change to docs/dashboard/                # auto-triggers on path
# OR merge a release-plz PR                          # auto-triggers on release
```

**Dashboard source**: `docs/dashboard/index.html` (sql.js-httpvfs, Chart.js)
- Loads `benchmarks.db` from same origin (downloaded from Releases at deploy time)
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