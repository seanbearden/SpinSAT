# Implementation Plan (Updated 2026-03-21)

## Phase 1: Core Solver (MVP) — COMPLETE
- Rust project, DIMACS parser (k-SAT), DMM equations (paper Eqs. 2-6)
- Forward Euler with adaptive dt, solution check, competition I/O, timeout

## Phase 1.5: Benchmarking Infrastructure — COMPLETE
- benchmark_suite.py, compare_results.py, perf_compare.py
- Kissat 4.0.4 + MiniSat 2.2.1 baselines, gratchk + check_sat verifiers
- JSON results with PAR-2 scoring, size breakdown, progress tracking

## Phase 2: Integrator Options — COMPLETE
- RK4 and Trapezoid integrators added
- CLI flags: --method euler|trapezoid|rk4, --seed, --timeout, --zeta
- Trapezoid 1.38x faster overall but high instance-to-instance variance

## Phase 3: Competition Heuristics — COMPLETE
- Random restart on stagnation detection (patience-based)
- Per-clause alpha_m adjustment every 10^4 time units
- Auto-zeta selection with log-linear interpolation by ratio
- Large suite: 24/26 -> 26/26 solved, PAR-2: 2113 -> 608

## Phase 4: Optimization — COMPLETE
- Single-pass derivative computation (fused L-value + min finding)
- Auto-zeta interpolation fix: PAR-2 608 -> 429
- Controlled A/B testing validated: AoS > SoA, separate loops > fused
- Performance findings documented (architecture-general, not Apple-specific)

## Phase 5: Competition Submission — COMPLETE
- Static Linux binary (x86_64-unknown-linux-musl, 593KB)
- build.sh + run.sh scripts
- 20 CDC-style benchmark instances (all verified by SpinSAT + MiniSat)
- .cargo/config.toml for cross-compilation

## Phase 6: Data-Driven Benchmarking & Versioning — COMPLETE
- **Automated versioning**: release-plz (zero config, no conventional commits)
  - .github/workflows/release-plz.yml + release-binary.yml
  - spinsat --version reads from Cargo.toml (env!("CARGO_PKG_VERSION"))
  - Cargo.toml synced to v0.4.0
  - CHANGELOG.md auto-generated via git-cliff
- **Benchmarks database**: benchmarks.db (SQLite)
  - Schema: runs, results, competition_results, instance_features + views
  - 31,809 instances snapshotted from meta.db (189 families, 61 tracks)
  - scripts/init_benchmarks_db.py for setup/refresh
- **Official benchmark recording**: --record flag on benchmark_suite.py
  - Auto-detects: solver version, git commit, dirty state, hardware, Rust version
  - Parses stderr for strategy, zeta, seed, restarts, method
  - --force flag for non-interactive/CI use
- **Competition data import**: scripts/import_competition_data.py
  - Anniversary Track support (5,355 instances x 28 solvers)
  - Generic CSV import for any competition year
- **GitHub Pages dashboard**: docs/dashboard/index.html
  - sql.js-httpvfs loads DB from GitHub Releases
  - 4 tabs: overview, version comparison, competition, SQL explorer
  - Datasette Lite link in README for ad-hoc queries
- **CI release assets**: binary auto-attached to GitHub Releases

## CI/CD & Quality — COMPLETE
- GitHub Actions: build + test + coverage (cargo-llvm-cov + nextest)
- Codecov: coverage upload, test results, component analytics
- 49 tests (32 unit + 17 numerical analysis integration)
- release-plz: auto versioning + CHANGELOG + crates.io publish
- release-binary: static musl binary on every release

## REMAINING (not phases — ongoing work)
- Register at organizers@satcompetition.org (deadline: April 19)
- System description document (1-2 pages IEEE, deadline: May 17)
- Test in competition Docker image
- Import competition reference data (Anniversary Track)
- Enable GitHub Pages (Settings → Pages → main/docs)
- Upload benchmarks.db to first release (when data is meaningful)
- Run official benchmark suite with --record against tagged release
- Instance difficulty profiling (SpinSAT vs competition baselines)
- Progress dashboard population with real data
- Test on real competition-scale instances (100K+ vars)
