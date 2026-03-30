# Integration Methods

## Available Methods (as of v0.5.3+)

### Euler (`-m euler`)
- Forward Euler, 1 RHS eval/step
- Simplest, least accurate, fastest per-step
- Analytical x_s update (exact solution with frozen C_m)

### Trapezoid (`-m trapezoid`)
- Heun's method, 2 RHS evals/step
- 2nd-order accuracy
- Analytical x_s with averaged C_m from both stages

### RK4 (`-m rk4`)
- Classical Runge-Kutta, 4 RHS evals/step
- 4th-order accuracy (capped ~1-2 near min() non-smoothness)
- Analytical x_s with RK4-weighted average C_m
- Good general-purpose, but expensive per step

### BS3 (`-m bs3`)
- Bogacki-Shampine 3(2), 3 RHS evals/step via FSAL
- 3rd-order with embedded 2nd-order error estimate
- PI step controller (rtol=0.5, relaxed for equilibrium-seeking)
- Step rejection/acceptance based on error estimate
- Best on qhid instances (5.0s vs RK4's 14.9s on qhid-200-5)

### Strang Splitting (`-m strang`)
- Half-step memories → Full-step voltages (RK4) → Half-step memories
- 2nd-order splitting accuracy
- Decouples memory stiffness from voltage integration
- Best on barthel instances (2.2s vs RK4's 2.5s on barthel-280)
- Enables much larger final_dt (0.82 vs 0.15)

## Key Innovation: Analytical x_s Update

All methods use exact solution for x_s ODE instead of numerical integration:
```
x_s_new = (x_s + ε) · exp(β(C_m - γ)·dt) - ε
```
This removes β=20 stiffness from step size constraint. The adaptive dt is
governed only by voltage dynamics (1/max|dv|).

## Optimization Flags

### Activity-Based Clause Skipping (`--activity-threshold <val>`)
- Skip voltage derivative contributions from satisfied clauses where C_m < threshold AND x_s < threshold
- Memory derivatives (dx_s, dx_l) still computed for all clauses
- Best with Strang on barthel: threshold=0.01 gives 2.2x speedup (1.1s vs 2.4s)
- **Hurts on qhid** — family-dependent, needs per-family tuning
- Default: 0.0 (disabled). Only applies to loop-based derivative engine.

### SER Convergence Acceleration (`--ser`)
- Switched Evolution Relaxation: grows dt when residual monotonically decreasing
- dt_new = dt_old × residual_old / residual_new (capped at 2x growth)
- Engages after 5 consecutive decreasing steps with max(C_m) < 0.4
- Allows dt up to 10× normal dt_max during convergence
- Minimal impact on small random instances (convergence phase too brief)
- Expected to help on larger structured instances

## Benchmark Results (barthel-280, seed=42)

| Method | Time | peak_xl_max | final_dt |
|--------|------|-------------|----------|
| Euler | 31.2s | 1.64e4 | 0.055 |
| Trapezoid | 3.9s | 3.78e3 | 0.092 |
| RK4 | 2.5s | 1.58e3 | 0.110 |
| BS3 | 5.0s | 3.27e3 | 0.126 |
| **Strang** | **2.2s** | 1.44e3 | 0.115 |
| **Strang+0.01** | **1.1s** | 8.87e2 | 0.397 |

## Benchmark Results (qhid-200-5, seed=42)

| Method | Time | peak_xl_max | final_dt |
|--------|------|-------------|----------|
| **BS3** | **5.0s** | 1.57e3 | 0.125 |
| Strang | 5.8s | 1.67e3 | 0.825 |
| Euler | 6.9s | 2.50e3 | 0.059 |
| Trapezoid | 8.1s | 2.61e3 | 0.099 |
| RK4 | 14.9s | 2.87e3 | 0.145 |
| Strang+0.01 | 16.1s | 3.08e3 | 0.222 |

## Family-Dependent Optimal Config
- **barthel (α_r≈4.3)**: `-m strang --activity-threshold 0.01`
- **qhid (α_r=5.5)**: `-m bs3` (threshold hurts)
- **General/unknown**: `-m strang` (safe universal improvement over RK4)
