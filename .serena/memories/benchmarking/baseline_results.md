# Phase 1 Baseline Results (2026-03-20)

All instances are planted random 3-SAT at ratio 4.3 (near complexity peak).
Tested on Apple M-series, single core.

## SpinSAT vs Kissat 4.0.4

| Suite | Vars | SpinSAT Solved | SpinSAT PAR-2 | Kissat Solved | Kissat PAR-2 | Gap |
|-------|------|---------------|---------------|---------------|--------------|-----|
| Small | 100-250 | 40/40 | 2.37 | 40/40 | 1.30 | 1.8x |
| Medium | 250-500 | 40/40 | 27.53 | 40/40 | 11.42 | 2.4x |
| Large | 500-2000 | 24/26 | 2113 | 26/26 | 63.66 | 33x |

## By Instance Size (Large Suite)

| Size | SpinSAT Solved | SpinSAT Median | Kissat Median | Ratio |
|------|---------------|----------------|---------------|-------|
| 250-500 | 10/10 | 0.62s | 0.06s | 10x |
| 500-1000 | 10/10 | 4.76s | 0.55s | 9x |
| >1000 | 4/6 | 233s | 8.3s | 28x |

## Key Observations
1. SpinSAT competitive up to ~500 vars
2. Gap explodes above 1000 vars due to high variance
3. Two timeouts: n1500_s1 (Kissat: 1.9s), n2000_s1 (Kissat: 18s)
4. Hard instance: n1000_s3 took 264s (Kissat: 7s) — 37x gap
5. Many instances solve FAST even at large N — the problem is getting STUCK on some

## Root Cause Analysis
- No restart mechanism → solver gets trapped in frustrated state
- Fixed seed (42) → no diversity in initial conditions
- No per-clause α_m adjustment → long-term memory doesn't adapt
- Forward Euler only → may need better integrator for stiff regions

## Highest-Impact Next Steps (Priority Order)
1. Random restarts (Phase 3) — directly addresses timeouts
2. Per-clause α_m heuristic (Phase 3) — helps stuck instances
3. Multiple random seeds (Phase 3) — reduces variance
4. ζ auto-tuning by ratio (Phase 3)
5. RK4 integrator (Phase 2) — may help stability at scale
