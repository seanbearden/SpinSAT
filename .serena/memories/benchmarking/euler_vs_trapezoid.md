# Euler vs Trapezoid Systematic Benchmark (2026-03-21)

## Protocol
- Same seed (-s 1), same instances, 120s timeout
- 40 planted 3-SAT instances (n=250,500,750,1000, ratio=4.3)
- Controlled comparison using Python time.time()

## Results
- Overall: Trapezoid 1.38x faster (PAR-2: 86.6 vs 119.9)
- Both solved 40/40 (no timeouts)
- Trapezoid takes ~2.3x fewer steps, costs ~2x per step

## Instance-Level Variance
- Speedup range: 0.03x to 12x
- Big Trapezoid wins: n500_s8 (12x), n250_s1/s3/s13 (3.5x)
- Big Euler wins: n500_s3 (Euler 36x faster), n750_s5 (Euler 25x faster)

## Conclusion
Neither method dominates — high instance-to-instance variance.
Trapezoid is net positive but unreliable on individual instances.

## Potential Strategy
Hybrid restart: start with one method, switch on stagnation.
Could capture best-of-both by trying each method on different restart attempts.
This would be a Phase 3 enhancement to the restart logic in solver.rs.
