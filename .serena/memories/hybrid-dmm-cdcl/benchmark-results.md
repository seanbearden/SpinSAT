# Hybrid DMM-CaDiCaL Benchmark Results (2026-03-22)

## Architecture
Bidirectional cooperation between SpinSAT DMM solver and CaDiCaL CDCL solver.
DMM provides: phase hints (best voltages) + frustrated variable assumptions (x_l → assume()).
CaDiCaL provides: fixed literals (unit learned clauses) + phase voltages back to DMM.
Adaptive time budget: DMM confidence decays with stagnation, giving CaDiCaL more time.

## Key A/B Result: tseitin_grid_n12_m12 (264 vars, 968 clauses, UNSAT)

| Mode | Mean Time | Solved (5 runs, 30s timeout) |
|---|---|---|
| **Adaptive hybrid (DMM + CaDiCaL + x_l)** | **24.25s** | **3/5** |
| CaDiCaL only (no DMM info) | 26.53s | 1/5 |

**Conclusion**: DMM's x_l clause difficulty gives CaDiCaL genuinely useful structural info.
The hybrid solves 3x more runs than CaDiCaL alone on this competition UNSAT instance.

## Instance Test Results

| Instance | Vars | Clauses | Type | Result | Notes |
|---|---|---|---|---|---|
| PHP(5,4) generated | 20 | 45 | UNSAT | Solved instantly | CaDiCaL trivial |
| PHP(8,7) generated | 56 | 204 | UNSAT | Solved (5.0s) | x_l seeded 5 vars |
| tseitin_grid_n12_m12 (comp) | 264 | 968 | UNSAT | 3/5 solved (30s) | Best result, x_l helps |
| tseitin_n188_d3 (comp) | 282 | 752 | UNSAT | UNKNOWN | Needs more budget |
| battleship-13-13-unsat (comp) | 169 | 1183 | UNSAT | UNKNOWN | Needs more budget |
| ramsey_3_6_18 (comp) | 153 | 19380 | UNSAT | UNKNOWN (3:34) | Hard for CaDiCaL too |
| harder-fphp-016-015 (comp) | 240 | 3496 | UNSAT | UNKNOWN (600s+) | Exponential for CDCL |
| Break_04_04 (comp) | 227 | 1106 | SAT | DMM solved (0.0s) | No fallback needed |

## Adaptive Budget Parameters (current tuning)
- confidence_decay: 0.2 per stagnant restart
- confidence_boost: 0.05 on genuine improvement
- min_dmm_share: 10% of remaining time
- Mid-solve CaDiCaL triggers: confidence < 0.6 AND consecutive_stagnant >= 2
- CaDiCaL budget per attempt: (1 - confidence) * 30% of remaining * 100K conflicts/sec
- Top-k frustrated vars: 10% of num_vars, capped at 50

## Bug Found and Fixed
- CaDiCaL fallback had no conflict limit → ran indefinitely on hard instances
- Fixed: conflict limit proportional to remaining wall-clock time
- CaDiCaL val() crashes if called after UNSAT → get_phases_as_voltages returns Option

## Competition Context
- Target: SAT Competition 2026 Experimental Track
- UNSAT proofs NOT required for Experimental Track
- Wrong answer = full disqualification
- Hardware: Intel Xeon Platinum 8368, 5000s timeout, 32GB RAM
