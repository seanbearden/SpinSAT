# SAT Competition 2025 — Hardest Instances Analysis

## Status: IN PROGRESS (2026-03-22)

## Competition Overview
- 400 instances: 172 SAT, 170 UNSAT, 58 unknown ground truth
- Winner: AE-Kissat-MAB (327/400, PAR-2: 2264.73, 5000s timeout)
- VBS: 347/400 — **53 instances unsolved by ANY solver**
- Hardware: Intel Xeon E3-1230 v5 @ 3.40GHz, 30GB RAM

## 2025 Had NO Experimental Track
- Only Main (Sequential) + Parallel tracks
- Experimental track is NEW in 2026 (SpinSAT's target)

## Categories of Hardest Unsolved Instances

### Scheduling (12 unsolved) — C/V ratio 19-66
- RoundRobin_n15_d13 through n18_d16 (8 instances, 1.3K-2.4K vars)
- MVRoundRobin_n14-n20 (4 instances, 3.6K-7.6K vars, C/V up to 66)
- All UNSAT. Author: Reeves (CMU)

### Ramsey Numbers (6 unsolved) — C/V ratio 40-1261!
- ramsey_3_6_18/19, ramsey_3_7_23/24, ramsey_4_4_18/19
- Very few vars (153-276) but astronomical C/V ratios
- Encode OPEN mathematical problems. Author: Anders (RPTU)

### Clique-Coloring (6 unsolved) — C/V 21-56
- clqcl_30_7_6 through clqcl_100_6_5 (300-6K vars)
- UNSAT, exponentially hard for resolution. Author: Anders

### Hardware Model Checking (5 unsolved) — massive
- pj2002_k500: 44.9M vars, 117M clauses (LARGEST unknown)
- pj2016_k100: 8.8M vars
- Several runs crashed at 20GB. Authors: Biere, Froleyks

### Mechanical Master-Key Systems (5 unsolved) — C/V ~2.0
- lockchart-group1-L200 through L220 (1.9M-2.2M vars)
- Novel domain, expected SAT. Author: Schreiber (KIT)

### Tseitin Formulas (4 unsolved) — provably hard for resolution
- tseitin_n188_d3: only 282 vars but HARD (provably exponential for resolution)
- tseitin_grid up to 319K vars
- Authors: Oertel, Yldirimoglu

### RISC Instruction Removal (3 unsolved) — 7M-13M vars
- oisc-subrv-and-nested-11/12/15. Author: Fleury

## SpinSAT Opportunity Analysis
- **Best candidates**: Ramsey, Tseitin, clique-coloring (small vars, high C/V)
  - SpinSAT's ODE dynamics may handle high C/V differently than CDCL
  - Tseitin with 282 vars is tiny — if SpinSAT can solve it, huge differentiator
- **Avoid initially**: Hardware model checking (too large, memory-bound)
- **Interesting**: Master-key (C/V ~2.0, novel domain, expected SAT)

## TODO
- [ ] Download 2025 benchmarks to shared storage
- [ ] Test SpinSAT on Ramsey and Tseitin instances specifically
- [ ] Analyze SpinSAT behavior near extreme C/V ratios
- [ ] Find per-instance timing data (not yet publicly available)
- [ ] Compare SpinSAT's polynomial scaling claim against these families
