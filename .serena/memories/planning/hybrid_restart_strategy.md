# Hybrid Restart Strategy — Design Plan

## Problem
Euler and Trapezoid each dominate on different instances. Speedup ranges 0.03x to 12x.
Neither method is universally better. We need to capture best-of-both.

## Proposed Design

### Approach: Method-Switching on Restart
When the solver detects stagnation and triggers a restart, alternate the integration
method. This lets us try both approaches on the same instance with minimal code change.

### Implementation Plan

#### 1. Add method field to SolverConfig restart logic (solver.rs)
```
restart 0: Euler (seed 42)
restart 1: Trapezoid (seed 42 + 7919)
restart 2: Euler (seed 42 + 2*7919)
restart 3: Trapezoid (seed 42 + 3*7919)
...
```
Alternating method on each restart gives both approaches a chance.

#### 2. Track per-method performance across restarts
Record (method, best_unsat, steps_to_stagnation) for each restart attempt.
If one method consistently reaches lower unsat counts, bias toward it.

#### 3. Adaptive method selection (advanced, future)
After N restarts, compute which method achieved better best_unsat on average.
Switch to using that method exclusively for remaining restarts.
This is a bandit-style explore/exploit strategy.

### Changes Required
- solver.rs: modify restart loop to alternate config.method
- solver.rs: create ScratchBuffers for both methods at init (minor memory cost)
- solver.rs: track per-method statistics
- main.rs: add --method auto flag (default to hybrid)

### Estimated Complexity
- Basic alternating: ~20 lines changed in solver.rs
- Adaptive selection: ~50 lines, needs per-method tracking struct
- Testing: run large suite with hybrid vs euler-only vs trap-only

### Expected Impact
- Should capture ~80% of instances where either method is best
- PAR-2 improvement: estimated 1.2-1.5x over Euler-only
- No regression possible (worst case = same as slower method, then restarts try the other)

### Risk
- Extra ScratchBuffers allocation for Trapezoid/RK4 even when using Euler on a given restart
- Minimal: ~3x the clause count in f64s for derivative buffers. For 100K clauses = ~2.4MB. Negligible.

### Dependencies
- None — can be implemented independently of Phase 5
- Should benchmark AFTER Phase 5 submission to avoid scope creep before deadline

### Priority
Medium — implement after Phase 5 competition packaging is complete.
The 1.38x Trapezoid advantage is real but inconsistent.
Competition deadline (April 19 registration) takes priority.
