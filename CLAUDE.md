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

**Automated via release-plz** — no manual version bumps, no conventional commits required.

- Version source of truth: `Cargo.toml` (read at compile time via `env!("CARGO_PKG_VERSION")`)
- **Never hardcode version strings** — use `env!("CARGO_PKG_VERSION")` in Rust code
- Push to main → release-plz opens a Release PR → merge → git tag + GitHub Release + crates.io publish
- Pre-compiled static binary attached to every GitHub Release via `release-binary.yml`

### CI/CD Workflows
- `.github/workflows/ci.yml` — build, test, coverage (cargo-llvm-cov + nextest + Codecov)
- `.github/workflows/release-plz.yml` — auto version bump + CHANGELOG + crates.io publish
- `.github/workflows/release-binary.yml` — attach static musl binary to GitHub Releases

### GitHub Secrets Required
- `CODECOV_TOKEN` — Codecov upload
- `CARGO_REGISTRY_TOKEN` — crates.io publish (scoped to spinsat crate)

## Benchmarking

### Database (`benchmarks.db`)

SQLite database in project root (gitignored, distributed via GitHub Releases). Contains:
- **Instance metadata**: 31,809 instances from SAT competition history (snapshot from `~/PycharmProjects/SpinSAT/meta.db`)
- **Benchmark results**: per-instance solve times with full reproducibility metadata
- **Competition reference**: SAT competition solve times for comparison
- **Views**: `best_times`, `version_comparison`

### Setup & Usage
```bash
# Initialize DB (one-time, snapshots meta.db)
python3 scripts/init_benchmarks_db.py

# Official recorded benchmark (auto-detects version, commit, hardware, params)
python3 scripts/benchmark_suite.py --suite large --record --tag v0.4.0

# Development run (JSON only, no DB recording)
python3 scripts/benchmark_suite.py --suite tiny --solver spinsat

# Import competition reference data
python3 scripts/import_competition_data.py --anni-csv <path>/anni-seq.csv

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

### Dashboard
- GitHub Pages: `docs/dashboard/index.html` (sql.js-httpvfs, loads DB from GitHub Releases)
- Datasette Lite: browser-based SQL explorer (link in README)

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