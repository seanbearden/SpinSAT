# SpinSAT

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

Tested on Apple M-series (single core). All solutions independently verified correct.

### SATLIB Uniform Random 3-SAT (ratio ≈ 4.26, near complexity peak)

| Instance Set | Variables | Clauses | Solved | Timeout | Max Time |
|-------------|-----------|---------|--------|---------|----------|
| UF20-91     | 20        | 91      | 5/5    | 0       | < 0.01s  |
| UF50-218    | 50        | 218     | 3/3    | 0       | < 0.01s  |
| UF100-430   | 100       | 430     | 3/3    | 0       | < 0.01s  |
| UF250-1065  | 250       | 1065    | 100/100| 0       | 64s      |

### Other Instances

| Instance | Variables | Clauses | Ratio | Time |
|----------|-----------|---------|-------|------|
| okgen-v500 (SAT 2002) | 500 | 2100 | 4.2 | 7.8s |
| Planted 3-SAT (generated) | 1000 | 4300 | 4.3 | 22.5s |

### Notable Hard Instance

`uf250-054.cnf` (250 vars, 1065 clauses) took 64s — the only instance in the UF250 batch exceeding 60s. All other 99 instances solved in under 31s. This instance warrants further investigation for tuning the integration parameters and restart heuristics.

## References

- [Main paper (open access)](https://www.nature.com/articles/s41598-020-76666-2)
- [Supplementary materials](https://doi.org/10.1038/s41598-020-76666-2) (linked from main paper)
- [SAT Competition 2026](https://satcompetition.github.io/2026/)
