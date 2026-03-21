# DMM Equations of Motion (Paper Version)

We implement the PAPER equations, not the modified competition variants.

## Three Coupled ODE Systems

### 1. Voltage dynamics (Eq. 2)
```
v̇_n = Σ_m x_{l,m} · x_{s,m} · G_{n,m} + (1 + ζ · x_{l,m}) · (1 - x_{s,m}) · R_{n,m}
```

### 2. Short-term memory (Eq. 3)
```
ẋ_{s,m} = β · (x_{s,m} + ε) · (C_m - γ)
```

### 3. Long-term memory (Eq. 4)
```
ẋ_{l,m} = α · (C_m - δ)
```

### Helper Functions

**Clause constraint** (Eq. 1):
```
C_m = ½ min[(1 - q_{i,m}·v_i), (1 - q_{j,m}·v_j), (1 - q_{k,m}·v_k)]
```

**Gradient-like function** (Eq. 5):
```
G_{n,m} = ½ · q_{n,m} · min[(1 - q_{j,m}·v_j), (1 - q_{k,m}·v_k)]
```
(min over the OTHER two literals in the clause, not n)

**Rigidity function** (Eq. 6):
```
R_{n,m} = ½(q_{n,m} - v_n)  when C_m = ½(1 - q_{n,m}·v_n)
R_{n,m} = 0                  otherwise
```
(nonzero only when variable n is the one closest to satisfying clause m)

## Variable Bounds
- v_n ∈ [-1, 1] — clamped after each step
- x_{s,m} ∈ [0, 1] — clamped after each step
- x_{l,m} ∈ [1, 10^4·M] — clamped after each step

## Parameters (Paper Defaults)
| Param | Value | Role |
|-------|-------|------|
| α | 5 | Long-term memory growth rate |
| β | 20 | Short-term memory growth rate |
| γ | 1/4 | Short-term memory threshold |
| δ | 1/20 | Long-term memory threshold |
| ε | 10⁻³ | Trapping rate |
| ζ | 10⁻¹ to 10⁻³ | Learning rate (lower for harder) |
| Δt | [2⁻⁷, 10³] | Adaptive time step range |

## Termination
Instance solved when C_m < 1/2 for ALL clauses m.
Assignment: y_n = TRUE if v_n > 0, FALSE if v_n < 0.

## Competition Heuristic (from Supplementary II.E)
Per-clause α_m, initially = 5. Every 10⁴ time units:
- median_xl = median(x_{l,m})
- If x_{l,m} > median: α_m *= 1.1
- If x_{l,m} ≤ median: α_m *= 0.9
- Clamp α_m ≥ 1
- If x_{l,m} hits max: reset x_{l,m} = 1, α_m = 1
