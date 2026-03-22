# Why DMM Struggles on Structured SAT Instances

## 4 Root Causes (Identified 2026-03-22)

### 1. Clause-Variable Ratio Mismatch
The DMM equations (Eqs. 2-4 in the paper) were tuned for ratio α_r ≈ 4.27 (3-SAT phase transition). The balance between gradient term (G_n,m) and rigidity term (R_n,m) is calibrated for this narrow band. Structured instances have wildly different ratios — srhd has ratio 204, logistics has ratio 84, Schur has ratio 36. The dynamics are poorly balanced outside the design range.

### 2. Long-Range Variable Correlations (Implication Chains)
In structured instances (planning, scheduling), variables are tightly coupled through long implication chains. A single variable flip can cascade across hundreds of clauses. DMM's continuous dynamics move ALL variables smoothly and simultaneously — there's no mechanism for the sharp "decision + unit propagation" cascade that CDCL does naturally. The smooth gradient can't navigate narrow corridors in the search space.

### 3. Deep Local Minima in Structured Energy Landscapes
Random 3-SAT near the phase transition has a relatively smooth energy landscape with one big basin of attraction. Structured instances have many deep local minima separated by high energy barriers. The DMM dynamics get trapped in these "near-solution" states. Evidence: mod2c consistently reaches 3-5 unsatisfied clauses out of 1740 (99.7% satisfied) but cannot close the gap across hundreds of restarts. The long-term memory (x_l) grows to weight frustrated clauses, but the barriers between minima are too high for the gradient to overcome.

### 4. Mixed Clause Widths
Many structured instances have 2-literal clauses (binary implications) alongside wider clauses. The constraint function C_m = ½ min(L_i) behaves differently for binary vs ternary clauses — binary clauses create sharper, more rigid constraints in the continuous domain. The gradient/rigidity balance designed for uniform 3-SAT breaks down when clause widths vary (k=2 through k=50+).

## Empirical Evidence (2026-03-22 benchmarks)

| Instance | Family | Vars | Ratio | Clause Widths | Best Unsat | Result |
|----------|--------|------|-------|---------------|------------|--------|
| mod2c-rand3bip-sat-170-1 | tseitin | 241 | 7.22 | 4,5,6 | 3/1740 | UNKNOWN |
| stb_588_138.apx_1 | argumentation | 1764→1097 | 6.52 | 2,3 | 15/8746 | UNKNOWN |
| WS_500_16_70_10.apx_0 | argumentation | 1500→998 | 7.67 | 2,3,8 | 15 | UNKNOWN |
| Schur_160_5_d40 | coloring | 750 | 36.21 | 2,3 | stagnates | UNKNOWN |

vs. uniform-random 3-SAT: 8/8 solved (250-500 vars, ratio 2.0-4.25)

## Research Directions
- Paper supplementary Section II discusses numerical implementation details — may have hints for adapting parameters to non-uniform clause widths
- Per-clause parameter adaptation (competition heuristic, Section II.E) partially addresses ratio mismatch but doesn't fix the structural trapping issue
- The CDCL fallback module (src/cdcl.rs) may help for structured instances where DPLL-style reasoning is essential
- Investigate whether clause-width-dependent scaling of gradient/rigidity terms could help
