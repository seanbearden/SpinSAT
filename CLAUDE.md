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

## Reference Materials

- `docs/efficient_solution_of_boolean_satisfiability_problems_with_digital_memcomputing.pdf` — Main paper
- `docs/efficient_solution_of_boolean_satisfiability_problems_with_digital_memcomputing-supplementary_materials.pdf` — Supplementary materials with full mathematical proofs, numerical implementation details (Section II), and competition instance results (Section II.E)