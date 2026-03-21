# Decision: Rust over C

## Date: 2026-03-20

## Decision
Use Rust as the implementation language instead of C.

## Rationale
- Memory safety prevents silent corruption when indexing 600K+ coupled ODE arrays
- Cargo ecosystem for testing, benchmarking, profiling
- Long-term maintainability beyond the competition
- Equivalent performance to C for numerical code

## Risks Mitigated
- No Rust on competition Docker image → pre-compile static musl binary
- No network access during build → vendor deps or pre-compile
- ODE library maturity → hand-write integrators (Euler, RK4, Trapezoid)

## Alternatives Considered
- C: safest for competition (zero deps), but manual memory management
- Go: GC pauses unacceptable for tight integration loops
- Python: orders of magnitude too slow
