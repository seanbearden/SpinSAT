use crate::formula::Formula;

/// DMM solver parameters (paper defaults).
pub struct Params {
    pub alpha: f64,
    pub beta: f64,
    pub gamma: f64,
    pub delta: f64,
    pub epsilon: f64,
    pub zeta: f64,
    pub dt_max: f64,
    pub dt_min: f64,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            alpha: 5.0,
            beta: 20.0,
            gamma: 0.25,
            delta: 0.05,
            epsilon: 1e-3,
            zeta: 1e-1,
            dt_max: 1024.0,      // 2^10
            dt_min: 1.0 / 128.0, // 2^-7
        }
    }
}

/// DMM state: voltages and memory variables.
pub struct DmmState {
    /// Voltage for each variable, v_n ∈ [-1, 1]
    pub v: Vec<f64>,
    /// Short-term memory per clause, x_{s,m} ∈ [0, 1]
    pub x_s: Vec<f64>,
    /// Long-term memory per clause, x_{l,m} ∈ [1, max_xl]
    pub x_l: Vec<f64>,
    /// Maximum value for x_l
    pub max_xl: f64,
}

impl DmmState {
    /// Initialize with random voltages and default memory values.
    pub fn new(formula: &Formula, seed: u64) -> Self {
        let n = formula.num_vars;
        let m = formula.num_clauses();

        // Simple xorshift64 PRNG for reproducibility without dependencies
        let mut rng_state = seed;
        let mut rand_f64 = || -> f64 {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            (rng_state as f64) / (u64::MAX as f64)
        };

        let v: Vec<f64> = (0..n).map(|_| 1.0 - 2.0 * rand_f64()).collect();
        let x_s: Vec<f64> = vec![0.0; m]; // Will be initialized to C_m in first step
        let x_l: Vec<f64> = vec![1.0; m];
        let max_xl = 1e4 * (m as f64);

        DmmState {
            v,
            x_s,
            x_l,
            max_xl,
        }
    }

    /// Initialize x_s to the clause constraint values (matching MATLAB InitializeVariables).
    pub fn init_short_memory(&mut self, formula: &Formula) {
        for m_idx in 0..formula.num_clauses() {
            self.x_s[m_idx] = clause_constraint(formula, m_idx, &self.v);
        }
    }
}

/// Compute clause constraint C_m (Eq. 1).
/// C_m = ½ min_k [(1 - q_{k,m} · v_k)] over all literals in clause m.
#[inline]
pub fn clause_constraint(formula: &Formula, m: usize, v: &[f64]) -> f64 {
    let clause = formula.clause(m);
    let mut min_val = f64::MAX;
    for &(var_idx, polarity) in clause {
        let val = 1.0 - polarity * v[var_idx];
        if val < min_val {
            min_val = val;
        }
    }
    0.5 * min_val
}

/// Derivatives output buffers.
pub struct Derivatives {
    pub dv: Vec<f64>,
    pub dx_s: Vec<f64>,
    pub dx_l: Vec<f64>,
    pub c_m: Vec<f64>,
}

impl Derivatives {
    pub fn new(num_vars: usize, num_clauses: usize) -> Self {
        Derivatives {
            dv: vec![0.0; num_vars],
            dx_s: vec![0.0; num_clauses],
            dx_l: vec![0.0; num_clauses],
            c_m: vec![0.0; num_clauses],
        }
    }
}

/// Compute all derivatives for the DMM system (Eqs. 2-6).
///
/// This is the hot path — called once per integration step.
pub fn compute_derivatives(
    formula: &Formula,
    state: &DmmState,
    params: &Params,
    derivs: &mut Derivatives,
) {
    let num_clauses = formula.num_clauses();

    // Zero voltage derivatives
    for d in derivs.dv.iter_mut() {
        *d = 0.0;
    }

    for m in 0..num_clauses {
        let clause = formula.clause(m);
        let k = clause.len();

        // Compute per-literal values: L_k = ½(1 - q_k · v_k)
        // and find the minimum (determines C_m and which literal is "closest")

        // Find the index of the literal with minimum L value (σ_m)
        let mut min_val = f64::MAX;
        let mut min_idx = 0;
        let mut second_min_val; // min over other literals (for gradient)

        // For k-SAT we need: C_m, which literal achieves the min, and min over others
        // We compute L values and track indices
        // For small k (3-8), a simple loop is efficient

        // Compute all L values
        let mut l_vals: [f64; 16] = [0.0; 16]; // support up to 16-SAT
        for (i, &(var_idx, polarity)) in clause.iter().enumerate() {
            l_vals[i] = 0.5 * (1.0 - polarity * state.v[var_idx]);
        }

        // Find minimum
        for i in 0..k {
            if l_vals[i] < min_val {
                min_val = l_vals[i];
                min_idx = i;
            }
        }

        // C_m = min of L values (already multiplied by 0.5)
        let c_m = min_val;
        derivs.c_m[m] = c_m;

        // Short-term memory derivative (Eq. 3): ẋ_{s,m} = β(x_{s,m} + ε)(C_m - γ)
        derivs.dx_s[m] = params.beta * (state.x_s[m] + params.epsilon) * (c_m - params.gamma);

        // Long-term memory derivative (Eq. 4): ẋ_{l,m} = α(C_m - δ)
        derivs.dx_l[m] = params.alpha * (c_m - params.delta);

        // Voltage derivatives (Eq. 2):
        // v̇_n = Σ_m [ x_l,m · x_s,m · G_{n,m} + (1 + ζ·x_l,m)·(1 - x_s,m) · R_{n,m} ]

        let xl = state.x_l[m];
        let xs = state.x_s[m];
        let fs = xl * xs; // gradient weight
        let rigidity_weight = (1.0 + params.zeta * xl) * (1.0 - xs);

        // For each literal position in this clause:
        for (i, &(var_idx, polarity)) in clause.iter().enumerate() {
            if i == min_idx {
                // This literal achieves the minimum → rigidity term applies
                // R_{n,m} = ½(q_{n,m} - v_n)
                let r_nm = 0.5 * (polarity - state.v[var_idx]);
                derivs.dv[var_idx] += rigidity_weight * c_m * r_nm;
            } else {
                // Gradient-like term for non-minimum literals
                // G_{n,m} = ½ · q_{n,m} · min over OTHER literals (excluding n)
                // The "min over others" when n is not the minimum literal
                // is just c_m (the overall min), since removing a non-min literal
                // doesn't change the min.

                // But we need min over literals OTHER than n.
                // If n is not the min literal, then min over others = c_m (min is still there).
                // So G_{n,m} = ½ · q_{n,m} · (2 · c_m) = q_{n,m} · c_m
                // Wait — let me re-derive from Eq. 5:
                // G_{n,m} = ½ · q_{n,m} · min_{j≠n}[(1 - q_j·v_j)]
                // The min over j≠n: if n is NOT the overall min, then the overall min is
                // still in the set, so min_{j≠n} = 2·c_m (since c_m = ½ · min of (1-q·v)).

                // Actually from the MATLAB code (derivative.m):
                // gradient = MN{4}*(c1.*fs) where c1 = min(L22, L23) for literal position 4
                // So for literal at position i, the gradient uses min of L values at OTHER positions.

                // Find min of L values excluding position i
                second_min_val = f64::MAX;
                for j in 0..k {
                    if j != i && l_vals[j] < second_min_val {
                        second_min_val = l_vals[j];
                    }
                }

                // G_{n,m} = ½ · q_{n,m} · min_{j≠n}(1 - q_j·v_j)
                // = ½ · q_{n,m} · (2 · second_min_val)  [since l_vals already has the ½]
                // = q_{n,m} · second_min_val
                // But wait, l_vals[j] = ½(1 - q_j·v_j), so min_{j≠n}(1-q_j·v_j) = 2·second_min_val
                // G_{n,m} = ½ · polarity · 2·second_min_val = polarity · second_min_val
                let g_nm = polarity * second_min_val;

                derivs.dv[var_idx] += fs * g_nm;
            }
        }

        // The rigidity term contribution for the min literal also includes the gradient part
        // from clauses where it is NOT the minimum. But wait — from Eq. 2 and the MATLAB,
        // the gradient term is x_l·x_s·G and rigidity is (1+ζ·x_l)·(1-x_s)·R.
        // For the min literal: G_{n,m} = 0 (from Eq. 5 definition, when the minimum
        // results in G=0 because the min over others might still give a value).
        //
        // Actually re-reading the paper and MATLAB more carefully:
        // In derivative.m, the gradient term for literal position 4 uses c1 = min(L22, L23),
        // which is the min over the OTHER two positions. This is computed for ALL positions,
        // including the one that achieves the overall minimum.
        //
        // The rigidity term (temp2) uses cmp1/cmp2/cmp3 which select ONLY the position
        // that achieves the minimum, and applies ttd = C3 * (1 - x_fast).
        //
        // Let me fix: gradient applies to ALL literals, rigidity only to the min literal.

        // CORRECTION: re-do the voltage derivatives properly
    }

    // Clear and recompute voltage derivatives with correct formulation
    for d in derivs.dv.iter_mut() {
        *d = 0.0;
    }

    for m in 0..num_clauses {
        let clause = formula.clause(m);
        let k = clause.len();

        let xl = state.x_l[m];
        let xs = state.x_s[m];
        let fs = xl * xs;
        let c_m = derivs.c_m[m];

        // Compute L values
        let mut l_vals: [f64; 16] = [0.0; 16];
        for (i, &(var_idx, polarity)) in clause.iter().enumerate() {
            l_vals[i] = 0.5 * (1.0 - polarity * state.v[var_idx]);
        }

        // Find which literal achieves the minimum
        let mut min_val = f64::MAX;
        let mut min_idx = 0;
        for i in 0..k {
            if l_vals[i] < min_val {
                min_val = l_vals[i];
                min_idx = i;
            }
        }

        // Gradient term: applies to ALL literal positions
        for (i, &(var_idx, polarity)) in clause.iter().enumerate() {
            // min over OTHER literals
            let mut min_others = f64::MAX;
            for j in 0..k {
                if j != i && l_vals[j] < min_others {
                    min_others = l_vals[j];
                }
            }
            // G_{n,m} = polarity * min_others (with ½ already in l_vals)
            let g_nm = polarity * min_others;
            derivs.dv[var_idx] += fs * g_nm;
        }

        // Rigidity term: applies ONLY to the literal achieving the minimum
        let (var_idx, polarity) = clause[min_idx];
        let r_nm = 0.5 * (polarity - state.v[var_idx]);
        let rigidity_weight = (1.0 + params.zeta * xl) * c_m * (1.0 - xs);
        derivs.dv[var_idx] += rigidity_weight * r_nm;
    }
}

/// Check if all clauses are satisfied (C_m < 0.5 for all m).
#[inline]
pub fn is_solved(c_m: &[f64]) -> bool {
    c_m.iter().all(|&c| c < 0.5)
}

/// Extract Boolean assignment from voltages by thresholding.
pub fn extract_assignment(v: &[f64]) -> Vec<bool> {
    v.iter().map(|&val| val > 0.0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::Formula;

    #[test]
    fn test_clause_constraint() {
        // Clause: (x1 ∨ ¬x2 ∨ x3), polarity = [+1, -1, +1]
        let f = Formula::new(3, vec![vec![1, -2, 3]]);
        // v = [1.0, -1.0, 0.5] → all literals satisfied
        let v = vec![1.0, -1.0, 0.5];
        let c = clause_constraint(&f, 0, &v);
        assert!(c < 0.5, "Clause should be satisfied, C_m = {}", c);

        // v = [-1.0, 1.0, -1.0] → all literals unsatisfied
        let v2 = vec![-1.0, 1.0, -1.0];
        let c2 = clause_constraint(&f, 0, &v2);
        assert!(c2 >= 0.5, "Clause should be unsatisfied, C_m = {}", c2);
    }

    #[test]
    fn test_extract_assignment() {
        let v = vec![0.5, -0.3, 0.0, 0.9, -0.1];
        let a = extract_assignment(&v);
        assert_eq!(a, vec![true, false, false, true, false]);
    }
}
