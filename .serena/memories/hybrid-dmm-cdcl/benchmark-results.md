# Hybrid DMM-CaDiCaL Benchmark Results (2026-03-22, final)

## Architecture
Bidirectional cooperation between SpinSAT DMM solver and CaDiCaL CDCL solver.
DMM provides: phase hints (best voltages) + frustrated variable assumptions (x_l via assume()).
CaDiCaL provides: fixed literals (unit learned clauses) + phase voltages back to DMM.
Adaptive time budget: DMM confidence decays with stagnation, giving CaDiCaL more time.

## Mixed Batch Test: 10 smallest competition instances, 60s timeout

| Instance | Vars | Cls | Time | Result | How |
|---|---|---|---|---|---|
| fixedbandwidth-eq-37 | 149 | 606 | 20.0s | UNSAT | Adaptive-CDCL |
| ramsey_3_6_18 | 153 | 19380 | timeout | UNKNOWN | - |
| battleship-13-13-unsat | 169 | 1183 | 19.7s | UNSAT | Adaptive-CDCL |
| homer11 | 220 | 1122 | 17.8s | UNSAT | Adaptive-CDCL |
| Break_04_04 | 227 | 1106 | 0.1s | SAT | DMM |
| harder-fphp-016-015 | 240 | 3496 | timeout | UNKNOWN | - |
| Break_triple_04_06 | 252 | 1163 | 0.1s | SAT | DMM |
| tseitin_grid_n12_m12 | 264 | 968 | 27.0s | UNSAT | Adaptive-CDCL |
| cliquecoloring_n14_k7_c6 | 273 | 5530 | 31.1s | UNSAT | Adaptive-CDCL |
| tseitin_n188_d3 | 282 | 752 | timeout | UNKNOWN | - |

**Result: 7/10 solved (2 SAT by DMM, 5 UNSAT by Adaptive-CDCL, 3 UNKNOWN)**
**No SAT regression**: DMM solves SAT instances in 0.1s — adaptive budget doesn't interfere.

## Tseitin tuning progression (tseitin_grid_n12_m12, 264 vars, 5 runs)

| Version | Solved | Mean |
|---|---|---|
| CaDiCaL only baseline | 1/5 | 26.53s |
| + phase hints + x_l | 3/5 | 24.25s |
| + adaptive budget (final) | 5/5 | 26.99s |

## Final Adaptive Budget Parameters
- Initial DMM share: 40% of timeout
- confidence_decay: 0.4 per stagnant restart
- confidence_boost: 0.05 on genuine improvement (skips first attempt)
- min_dmm_share: 5% of remaining time
- Mid-solve CaDiCaL triggers: confidence < 0.7 AND consecutive_stagnant >= 1
- CaDiCaL mid-solve budget: (1 - confidence) * 50% of remaining * 500K conflicts/sec
- Final fallback budget: remaining_time * 500K conflicts/sec (min 500K)
- Top-k frustrated vars assumed: 10% of num_vars, capped at 50

## Unsolved instances (need 5000s competition timeout testing)
- ramsey_3_6_18: 153 vars, 19380 clauses — Ramsey theory, hard for CDCL
- harder-fphp-016-015: 240 vars, 3496 clauses — functional pigeonhole, exponential resolution
- tseitin_n188_d3: 282 vars, 752 clauses — Tseitin formula on random degree-3 graph

## Bugs Found and Fixed
- CaDiCaL fallback had no conflict limit — ran indefinitely (fixed)
- CaDiCaL val() crashes after UNSAT — get_phases_as_voltages returns Option
- First DMM attempt always "improves" from num_clauses — don't boost confidence
- x_l transfer was missing from fallback path — now both paths seed frustrated vars
