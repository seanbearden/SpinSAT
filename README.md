# SpinSAT

[![CI](https://github.com/seanbearden/SpinSAT/actions/workflows/ci.yml/badge.svg)](https://github.com/seanbearden/SpinSAT/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/seanbearden/SpinSAT/branch/main/graph/badge.svg)](https://codecov.io/gh/seanbearden/SpinSAT)

A dynamic SAT solver based on digital memcomputing machines (DMMs), written in Rust.

## Introduction

SpinSAT solves Boolean satisfiability (SAT) problems by mapping them onto a system of coupled ordinary differential equations (ODEs). Instead of discrete search (DPLL/CDCL), it uses continuous-time dynamics where Boolean variables become voltages and memory variables guide the system toward satisfying assignments.

The approach is based on the research paper:
> S.R.B. Bearden, Y.R. Pei, M. Di Ventra. "Efficient Solution of Boolean Satisfiability Problems with Digital MemComputing." *Scientific Reports* 10, 19741 (2020).
> https://doi.org/10.1038/s41598-020-76666-2

Key results from the paper:
- **Power-law scaling** of integration steps for hard planted-solution 3-SAT instances (CDC class), compared to exponential scaling for WalkSAT and SID
- **No chaotic dynamics** — the system avoids exponential energy growth, unlike previous dynamical approaches
- **Collective variable updates** — long-range order enables the system to explore the solution space efficiently
- Successfully solved all tested competition instances from the 2017 and 2018 SAT Competition Random Tracks within the 5000-second timeout

## SAT Competition 2026

SpinSAT targets the **Experimental Track** of the [International SAT Competition 2026](https://satcompetition.github.io/2026/). This track is designed for solvers using unconventional techniques not yet supported by certificate generation — a natural fit for a physics-inspired ODE solver.

- **Track**: Experimental (no UNSAT proof certificates required)
- **Timeout**: 5000 seconds per instance
- **Environment**: Ubuntu 24.04, Intel Xeon Platinum 8368, 32 GB RAM, single-threaded
- **Solver type**: Incomplete (can find SAT assignments, cannot prove UNSAT)

### Key Deadlines
- **April 19, 2026**: Solver registration + benchmark submission
- **April 26, 2026**: Solver code submission
- **May 17, 2026**: System description document

## How It Works

1. **Parse** a SAT instance in DIMACS CNF format into a polarity matrix `Q`
2. **Initialize** continuous voltages `v_n ∈ [-1, 1]` and memory variables `x_{s,m}`, `x_{l,m}`
3. **Integrate** the DMM equations of motion using forward-Euler with adaptive time step
4. **Check** if all clause constraints `C_m < 1/2` — if so, threshold voltages to obtain a Boolean assignment
5. **Output** SAT + assignment, or UNKNOWN if timeout is reached

## Building

Requires Rust toolchain (1.75+):

```bash
cargo build --release
```

For the competition, a pre-compiled static Linux binary is included for environments without Rust.

## Usage

```bash
./target/release/spinsat <instance.cnf>
```

Output follows the SAT competition standard format:
```
s SATISFIABLE
v 1 -2 3 -4 5 0
```

## Benchmark Results

Results tracked in `results/` as JSON with PAR-2 scoring. Run benchmarks with:

```bash
python3 scripts/benchmark_suite.py --suite large --solver spinsat --solver kissat --timeout 300 --tag mytag
python3 scripts/compare_results.py --by-size
```

### SpinSAT vs Kissat 4.0.4 (planted 3-SAT, ratio 4.3)

| Suite | Vars | SpinSAT Solved | SpinSAT PAR-2 | Kissat PAR-2 |
|-------|------|---------------|---------------|--------------|
| Small | 100-250 | 40/40 | 2.4 | 1.3 |
| Medium | 250-500 | 40/40 | 60 | 11 |
| Large | 500-2000 | 26/26 | 429 | 64 |

### Progress Across Phases

| Phase | Large PAR-2 | Solved | Key Change |
|-------|-------------|--------|------------|
| Phase 1 (baseline) | 2113 | 24/26 | Core Euler solver |
| Phase 3 (heuristics) | 608 | 26/26 | Restarts + per-clause α_m |
| Phase 4 (optimized) | 429 | 26/26 | Auto-zeta + single-pass derivatives |

## Competition Submission TODO

- [ ] Register at organizers@satcompetition.org (deadline: April 19, 2026)
- [ ] Submit 20 benchmark instances (deadline: April 19, 2026)
  - [ ] Verify no instance solvable by MiniSat in under 60s (competition requirement)
  - [ ] Verify all solvable by SpinSAT within 1 hour
- [ ] Final solver code submission (deadline: April 26, 2026)
- [ ] System description document, 1-2 pages, IEEE Proceedings style PDF (deadline: May 17, 2026)
- [ ] Test solver in competition Docker image (`registry.gitlab.com/sosy-lab/benchmarking/competition-scripts/user:latest`)
- [ ] Make repository public after submission deadline

## Development Tools

| Tool | Version | Purpose |
|------|---------|---------|
| Rust (rustc) | 1.94.0 | Solver implementation language |
| Kissat | 4.0.4 | CDCL baseline solver for comparison |
| MiniSat | 2.2.1 | Benchmark difficulty validation |
| gratchk | (MLton build) | Competition-grade SAT certificate verifier |
| check_sat | (custom C) | Fast local solution verifier |
| gtimeout | (coreutils) | macOS timeout command |

## References

- [Main paper (open access)](https://www.nature.com/articles/s41598-020-76666-2)
- [Supplementary materials](https://doi.org/10.1038/s41598-020-76666-2) (linked from main paper)
- [SAT Competition 2026](https://satcompetition.github.io/2026/)
