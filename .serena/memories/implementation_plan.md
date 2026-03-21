# Implementation Plan

## Phase 1: Core Solver (MVP)
1. Rust project setup (cargo init, project structure)
2. DIMACS CNF parser — generalized to k-SAT
3. Sparse polarity matrix representation (CSR format)
4. DMM derivative computation (paper Eqs. 2-6)
5. Forward Euler integrator with adaptive time step
6. Solution check (C_m < 0.5 for all m) and assignment extraction
7. Competition I/O (stdout: s SATISFIABLE / v ... 0)
8. Timeout handling (5000s wall clock)

## Phase 2: Integrator Options
9. RK4 integrator
10. Trapezoid (Heun's) integrator
11. Command-line flag to select method

## Phase 3: Competition Heuristics
12. Per-clause α_m adjustment (paper Supplementary II.E)
13. Random restart on timeout
14. ζ parameter auto-selection based on clause-to-variable ratio

## Phase 4: Optimization & Scaling
15. Profile on large instances (1000+ variables)
16. Memory layout optimization for cache performance
17. SIMD for vectorized clause evaluation (if beneficial)
18. Test on SAT Competition 2017/2018 Random Track instances

## Phase 5: Competition Submission
19. Cross-compile static Linux binary (musl target)
20. Write build.sh and run.sh
21. Test in competition Docker image
22. Write system description document (1-2 pages)
23. Submit 20 benchmark instances
24. Register by April 19, submit by April 26

## Architecture
```
src/
  main.rs          — CLI entry point, timeout, output
  parser.rs        — DIMACS CNF parser
  formula.rs       — Polarity matrix, clause/variable storage
  dmm.rs           — DMM state (voltages, memories) and derivative
  integrator.rs    — Euler, RK4, Trapezoid methods
  solver.rs        — Main solve loop, termination check
```
