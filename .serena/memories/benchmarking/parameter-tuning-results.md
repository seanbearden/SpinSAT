# Parameter Tuning: xl_decay and restart_noise (2026-03-22)

## Test Setup
- 6 hard 3-SAT instances (360-400 vars, ratio 4.26, random_2007 track)
- Cycling restart mode, timeout 60s, seed 42
- 20 parameter combinations: decay [0.0, 0.1, 0.3, 0.5, 0.7] x noise [0.05, 0.1, 0.2, 0.3]

## Top 5 Configs (by PAR-2)

| Decay | Noise | Solved | PAR-2 |
|---|---|---|---|
| **0.5** | **0.05** | **6/6** | **149.5** |
| 0.7 | 0.20 | 6/6 | 149.8 |
| 0.3 | 0.20 | 6/6 | 169.1 |
| 0.3 | 0.10 | 5/6 | 215.7 |
| 0.3 | 0.30 | 5/6 | 224.6 |

## Bottom 5 Configs

| Decay | Noise | Solved | PAR-2 |
|---|---|---|---|
| 0.5 | 0.30 | 4/6 | 367.9 |
| 0.0 | 0.10 | 3/6 | 394.5 |
| 0.1 | 0.30 | 3/6 | 411.2 |

## Key Findings
1. **Higher decay is dramatically better**: decay=0.0 → 4/6 solved; decay=0.5 → 6/6 solved
2. **Retaining more x_l memory helps**: the clause difficulty history is the most valuable signal
3. **Low noise is generally better**: noise=0.05 outperforms 0.1-0.3 at most decay levels
4. **Sweet spot: decay=0.5, noise=0.05** (PAR-2: 149.5, 6/6 solved)
5. **Two near-optimal configs**: (0.5, 0.05) and (0.7, 0.20) are within 0.3 PAR-2
6. **Previous defaults were suboptimal**: decay=0.3, noise=0.1 scored 215.7 (5/6)

## Updated Defaults
- xl_decay: 0.3 → 0.5
- restart_noise: 0.1 → 0.05

## Interpretation
- x_l encodes per-clause frustration history — the more you retain, the better the warm restart
- Low noise means the voltage initialization from best-ever assignment is already good
- Too much noise corrupts the voltage signal; too little prevents exploration (but cycling's
  anti-phase restart handles exploration)
- The cycling pattern (Cold→Warm→Warm→AntiPhase) provides enough diversity that warm
  restarts can afford to be conservative (low noise, high memory retention)

## Caveat
Small sample (6 instances, single instance family). Needs validation on:
- Larger instance sets (50+)
- Different instance families (not just uniform-random)
- Different variable counts (500+, 1000+)
- Different ratios (not just 4.26)
