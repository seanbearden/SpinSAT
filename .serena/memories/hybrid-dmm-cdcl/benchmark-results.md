# Hybrid DMM-CaDiCaL Benchmark Results (2026-03-22, updated)

## Architecture
Bidirectional cooperation between SpinSAT DMM solver and CaDiCaL CDCL solver.
DMM provides: phase hints (best voltages) + frustrated variable assumptions (x_l via assume()).
CaDiCaL provides: fixed literals (unit learned clauses) + phase voltages back to DMM.
Adaptive time budget: DMM confidence decays with stagnation, giving CaDiCaL more time.

## Key Result: 5/5 solved on tseitin_grid_n12_m12 (264 vars, 968 clauses, UNSAT)

### Tuning progression:
| Version | Solved | Mean | Key change |
|---|---|---|---|
| Fixed 50/50 split | 1/5 (CaDiCaL only baseline) | 26.53s | No DMM info |
| Fixed 50/50 + x_l | 3/5 | 24.25s | Added x_l transfer |
| Adaptive decay=0.2 | 3/5 | 24.99s | First adaptive attempt |
| Adaptive decay=0.3 | 3/5 | 24.30s | Faster decay |
| Adaptive decay=0.4, trigger=1 | 2/5 | 24.79s | Too many small CaDiCaL attempts |
| 40% initial DMM, decay=0.4 | 2/5 | 21.08s | Faster but still budget-limited |
| **Final: 40% DMM + 500K conf/sec** | **5/5** | **26.99s** | Higher conflict budget estimate |

### Final 5/5 run details:
- Run 1: 16.5s — Mid-solve adaptive CaDiCaL proved UNSAT
- Run 2: 30.4s — Final fallback (6.1M conflicts)
- Run 3: 14.0s — Mid-solve adaptive CaDiCaL proved UNSAT
- Run 4: 39.1s — Final fallback (6.3M conflicts)
- Run 5: 34.4s — Final fallback (6.3M conflicts)

## Final Adaptive Budget Parameters
- Initial DMM share: 40% of timeout
- confidence_decay: 0.4 per stagnant restart
- confidence_boost: 0.05 on genuine improvement (skips first attempt)
- min_dmm_share: 5% of remaining time
- Mid-solve CaDiCaL triggers: confidence < 0.7 AND consecutive_stagnant >= 1
- CaDiCaL budget: (1 - confidence) * 50% of remaining * 500K conflicts/sec
- Final fallback budget: remaining_time * 500K conflicts/sec (min 500K)
- Top-k frustrated vars: 10% of num_vars, capped at 50

## All Instance Test Results

| Instance | Vars | Clauses | Type | Result | Notes |
|---|---|---|---|---|---|
| PHP(5,4) generated | 20 | 45 | UNSAT | Solved instantly | Trivial |
| PHP(8,7) generated | 56 | 204 | UNSAT | Solved (5.0s) | x_l seeded 5 vars |
| tseitin_grid_n12_m12 (comp) | 264 | 968 | UNSAT | **5/5 solved** | Best result |
| tseitin_n188_d3 (comp) | 282 | 752 | UNSAT | UNKNOWN | Needs more budget |
| battleship-13-13-unsat (comp) | 169 | 1183 | UNSAT | UNKNOWN | Needs more budget |
| ramsey_3_6_18 (comp) | 153 | 19380 | UNSAT | UNKNOWN | Hard for CaDiCaL too |
| harder-fphp-016-015 (comp) | 240 | 3496 | UNSAT | UNKNOWN | Exponential for CDCL |
| Break_04_04 (comp) | 227 | 1106 | SAT | DMM solved (0.0s) | No fallback needed |

## Bugs Found and Fixed
- CaDiCaL fallback had no conflict limit — ran indefinitely (fixed: proportional to remaining time)
- CaDiCaL val() crashes after UNSAT — get_phases_as_voltages returns Option
- First DMM attempt always "improves" from num_clauses — don't boost confidence on first attempt
- x_l transfer was missing from fallback path — now both paths seed frustrated vars
