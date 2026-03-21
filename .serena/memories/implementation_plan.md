# Implementation Plan (Updated 2026-03-20)

## Phase 1: Core Solver (MVP) — COMPLETE
1. Rust project setup (cargo init, project structure)
2. DIMACS CNF parser — generalized to k-SAT
3. Polarity matrix representation
4. DMM derivative computation (paper Eqs. 2-6)
5. Forward Euler integrator with adaptive time step
6. Solution check and assignment extraction
7. Competition I/O (s SATISFIABLE / v ... 0)
8. Timeout handling (5000s wall clock)

## Phase 1.5: Benchmarking Infrastructure — COMPLETE
- benchmark_suite.py with suites (tiny/small/medium/large)
- JSON results with PAR-2 scoring
- Kissat 4.0.4 baseline, gratchk verifier, check_sat.c
- compare_results.py with size breakdown and progress tracking
- Baseline results recorded

## Phase 2: Integrator Options — DEFERRED
9. RK4 integrator
10. Trapezoid integrator
11. CLI flags (--method, --seed, --timeout)
Rationale: restarts (Phase 3) address root cause better than integrator improvements

## Phase 3: Competition Heuristics — HIGHEST PRIORITY (NEXT)
12. Random restart on stagnation (biggest impact on timeouts)
13. Multiple random seeds / restart with new ICs
14. Per-clause alpha_m adjustment (paper Supplementary II.E)
15. Zeta auto-selection based on clause-to-variable ratio

## Phase 4: Optimization and Scaling
16. Profile on large instances (1000+ variables)
17. Memory layout optimization (SoA vs AoS)
18. Sparse matrix representation for gradient
19. Test on real SAT Competition instances

## Phase 5: Competition Submission
20. Cross-compile static Linux binary (musl target)
21. Write build.sh and run.sh
22. Test in competition Docker image
23. System description document (1-2 pages, IEEE)
24. Submit 20 benchmark instances
25. Register by April 19, submit by April 26

## Priority Decision
Phase 3 before Phase 2 because baseline data shows the solver gets STUCK
on some instances (264s vs 7s Kissat), not that it is slow per-step.
Restarts directly address the high-variance timeouts.
