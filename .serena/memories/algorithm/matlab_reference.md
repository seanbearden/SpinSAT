# Reference MATLAB Implementations

## Paper-Matching Code (USE THIS)
Location: ~/Downloads/v9_large_ratio/
- `derivative.m` — Core RHS: txs = alpha*(C3-delta), txf = beta*(x_fast+epsilon).*(C3-gamma)
- `SeanMethod.m` — Forward Euler with adaptive dt = max(min(dt_max, max_v/max(|dV|)), dt_min)
- `RungeKutta4.m` — RK4 integrator with clamping
- `Trapezoid.m` — Trapezoidal (Heun's) method
- `ImportData.m` — DIMACS CNF parser using sparse matrices
- `InitializeVariables.m` — Random V ∈ [-1,1], x_fast = C_m, x_slow = 1
- `main.m` — Orchestrator with paper params (α=5, β=20, γ=0.25, δ=0.05, ε=10⁻³)

## Key Implementation Details from MATLAB
- Polarity stored as sparse matrices MN{4}, MN{5}, MN{6} (one per literal position in clause)
- mn4, mn5, mn6 are pre-divided by 2 to reduce ops in inner loop
- mp4 = abs(mn4) for unsigned version
- mi4, mi5, mi6 are row indices mapping clauses to variables
- Gradient computed via sparse matrix-vector multiply: MN{4}*(c23.*fs)
- Fixed dt version: dt = min(dt_init, 10/max(x_slow.*x_fast))

## Modified Competition Variants (for future reference, NOT initial implementation)
Location: ~/Documents/DiVentraGroup/Factorization/Spin_SAT/clean_versions/
- SpinSAT_v1_0.m — Multiplicative x_slow growth, sqrt damping, different params
- SpinSAT_v1_1.m — MNF sparse optimization variant
- SpinSAT_k5_v1_0.m — Generalized to 5-SAT with k*(k-1)/2 pairwise comparisons
- SpinSAT_smart_restart.m — Clause removal/restart heuristic (interesting hybrid)
- cnf_preprocess.m — k-SAT parser (detects clause width, builds sparse matrices)

## CRITICAL: Modified equations differ from paper
- x_slow: `x_slow += α·x_slow·(x_fast - C/sym_break)` (multiplicative, NOT additive)
- x_fast: `x_fast += β·(x_fast·(1-x_fast))^0.5·(C - x_fast/sym_break)` (sqrt damping)
- Different params: α=0.01, β=0.5, sym_break=2.5
- These may perform better on competition instances but are NOT the paper equations
