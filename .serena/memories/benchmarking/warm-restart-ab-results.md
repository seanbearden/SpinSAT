# Warm Restart A/B Results (2026-03-22)

## Test Setup
- 20 hard 3-SAT instances from random_2007 (360-500 vars, ratio 4.25-4.26)
- Timeout: 120s, seed: 42
- SpinSAT v0.5.0 with preprocessing enabled
- Hardware: Apple M-series (ARM64)

## Results

| Mode | Solved | Timeouts | PAR-2 |
|---|---|---|---|
| Cold | 16/20 | 4 | 1379.8 |
| Warm | 17/20 | 3 | 1133.7 |
| **Cycling** | **19/20** | **1** | **864.8** |

## Key Findings
1. **Cycling mode solved 3 instances cold couldn't** (3 NEW! results)
2. **PAR-2 improved 37%** (1379.8 → 864.8) with cycling vs cold
3. **Best individual speedup: 5.57x** (warm on v400-S141590207, 77.9s → 14.0s)
4. Warm needed fewer restarts than cold (1 vs 3 on several instances)
5. Anti-phase restart (part of cycling) found solutions cold and warm missed
6. On easy instances (solved without restarts), all modes are equivalent

## Instance-Level Highlights
- v450-S216896591: cold T/O, warm 85.0s (2R), cycling 61.6s (2R) — NEW
- v400-S141590207: cold 77.9s (3R), warm 14.0s (1R), cycling 14.2s (1R) — 5.57x
- v400-S1752604460: cold T/O, warm T/O, cycling 101.8s (5R) — NEW (anti-phase found it)
- v500-S44928635: cold T/O, warm T/O, cycling 116.8s (5R) — NEW

## Implications
- **Cycling should be the default restart mode** for competition
- The x_l decay transfer preserves clause difficulty information effectively
- Anti-phase restart enables cluster-hopping at phase transition
- These results support the "Informed Restarts for DMMs" paper concept
- Need larger-scale validation on more instances and different instance families
