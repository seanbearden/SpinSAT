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

impl Params {
    /// Auto-select zeta based on clause-to-variable ratio.
    /// From the paper: ζ=10⁻¹ for high ratio (≥6), ζ=10⁻² for ratio≈5, ζ=10⁻³ near α_r≈4.27.
    /// Uses log-linear interpolation for smooth transition.
    pub fn with_auto_zeta(mut self, ratio: f64) -> Self {
        self.zeta = if ratio >= 6.0 {
            1e-1
        } else if ratio >= 5.0 {
            // Interpolate between 1e-2 (ratio=5) and 1e-1 (ratio=6)
            let t = ratio - 5.0; // 0..1
            10f64.powf(-2.0 + t)
        } else if ratio >= 4.5 {
            // Interpolate between 1e-3 (ratio=4.5) and 1e-2 (ratio=5)
            let t = (ratio - 4.5) * 2.0; // 0..1
            10f64.powf(-3.0 + t)
        } else {
            1e-3
        };
        self
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
    /// Per-clause alpha_m for competition heuristic
    pub alpha_m: Vec<f64>,
    /// Integration time (sum of dt)
    pub t: f64,
    /// Time of last α_m adjustment
    pub last_alpha_adjust_t: f64,
}

impl DmmState {
    /// Initialize with random voltages and default memory values.
    pub fn new(formula: &Formula, seed: u64) -> Self {
        let n = formula.num_vars;
        let m = formula.num_clauses();

        let v = Self::random_voltages(n, seed);
        let x_s: Vec<f64> = vec![0.0; m];
        let x_l: Vec<f64> = vec![1.0; m];
        let max_xl = 1e4 * (m as f64);
        let alpha_m = vec![5.0; m]; // initially = α = 5

        DmmState {
            v,
            x_s,
            x_l,
            max_xl,
            alpha_m,
            t: 0.0,
            last_alpha_adjust_t: 0.0,
        }
    }

    /// Generate random voltages in [-1, 1] using xorshift64.
    fn random_voltages(n: usize, seed: u64) -> Vec<f64> {
        let mut rng_state = seed;
        let mut rand_f64 = || -> f64 {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            (rng_state as f64) / (u64::MAX as f64)
        };
        (0..n).map(|_| 1.0 - 2.0 * rand_f64()).collect()
    }

    /// Initialize x_s to the clause constraint values (matching MATLAB InitializeVariables).
    pub fn init_short_memory(&mut self, formula: &Formula) {
        for m_idx in 0..formula.num_clauses() {
            self.x_s[m_idx] = clause_constraint(formula, m_idx, &self.v);
        }
    }

    /// Restart with new random voltages, reset memories.
    /// Keeps alpha_m (learned clause difficulty) across restarts.
    pub fn restart(&mut self, formula: &Formula, seed: u64) {
        let n = formula.num_vars;
        let m = formula.num_clauses();

        self.v = Self::random_voltages(n, seed);
        self.x_s = vec![0.0; m];
        self.x_l = vec![1.0; m];
        self.t = 0.0;
        self.last_alpha_adjust_t = 0.0;
        self.init_short_memory(formula);
    }

    /// Per-clause α_m adjustment heuristic (paper Supplementary II.E).
    /// Called every 10⁴ time units.
    pub fn adjust_alpha_m(&mut self) {
        let m = self.x_l.len();
        if m == 0 {
            return;
        }

        // Compute median of x_l values
        let mut sorted_xl: Vec<f64> = self.x_l.clone();
        sorted_xl.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = sorted_xl[m / 2];

        for i in 0..m {
            if self.x_l[i] > median {
                self.alpha_m[i] *= 1.1;
            } else {
                self.alpha_m[i] *= 0.9;
            }
            // Clamp α_m ≥ 1
            if self.alpha_m[i] < 1.0 {
                self.alpha_m[i] = 1.0;
            }
            // If x_l hits max, reset both
            if self.x_l[i] >= self.max_xl {
                self.x_l[i] = 1.0;
                self.alpha_m[i] = 1.0;
            }
        }

        self.last_alpha_adjust_t = self.t;
    }
}

/// Compute clause constraint C_m (Eq. 1).
#[inline]
pub fn clause_constraint(formula: &Formula, m: usize, v: &[f64]) -> f64 {
    let (start, width) = formula.clause_range(m);
    let mut min_val = f64::MAX;
    for i in 0..width {
        let pos = start + i;
        let val = 1.0 - formula.polarity(pos) * v[formula.var_idx(pos)];
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
/// Uses per-clause alpha_m instead of global alpha for long-term memory.
/// Single-pass implementation: computes C_m, memory derivatives, and voltage
/// derivatives together to avoid redundant L-value computation.
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
        let (start, k) = formula.clause_range(m);

        // Compute L values and find min/second-min in fused loop
        let mut min_val = f64::MAX;
        let mut min_local_idx: usize = 0;
        let mut second_min_val = f64::MAX;
        let mut l_vals: [f64; 16] = [0.0; 16];

        for i in 0..k {
            let pos = start + i;
            let l = 0.5 * (1.0 - formula.polarity(pos) * state.v[formula.var_idx(pos)]);
            l_vals[i] = l;
            if l < min_val {
                second_min_val = min_val;
                min_val = l;
                min_local_idx = i;
            } else if l < second_min_val {
                second_min_val = l;
            }
        }

        let c_m = min_val;
        derivs.c_m[m] = c_m;

        // Memory derivatives
        derivs.dx_s[m] = params.beta * (state.x_s[m] + params.epsilon) * (c_m - params.gamma);
        derivs.dx_l[m] = state.alpha_m[m] * (c_m - params.delta);

        // Voltage derivatives — fused gradient + rigidity
        let xl = state.x_l[m];
        let xs = state.x_s[m];
        let fs = xl * xs;

        // Gradient term for each literal
        for i in 0..k {
            let pos = start + i;
            let var_idx = formula.var_idx(pos);
            let polarity = formula.polarity(pos);
            let min_others = if i == min_local_idx {
                second_min_val
            } else {
                min_val
            };
            derivs.dv[var_idx] += fs * polarity * min_others;
        }

        // Rigidity term: only for the min literal
        let min_pos = start + min_local_idx;
        let var_idx = formula.var_idx(min_pos);
        let polarity = formula.polarity(min_pos);
        let r_nm = 0.5 * (polarity - state.v[var_idx]);
        derivs.dv[var_idx] += (1.0 + params.zeta * xl) * c_m * (1.0 - xs) * r_nm;
    }
}

/// Check if all clauses are satisfied (C_m < 0.5 for all m).
#[inline]
pub fn is_solved(c_m: &[f64]) -> bool {
    c_m.iter().all(|&c| c < 0.5)
}

/// Count unsatisfied clauses.
#[inline]
pub fn count_unsat(c_m: &[f64]) -> usize {
    c_m.iter().filter(|&&c| c >= 0.5).count()
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
        let f = Formula::new(3, vec![vec![1, -2, 3]]);
        let v = vec![1.0, -1.0, 0.5];
        let c = clause_constraint(&f, 0, &v);
        assert!(c < 0.5, "Clause should be satisfied, C_m = {}", c);

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

    #[test]
    fn test_adjust_alpha_m() {
        let f = Formula::new(3, vec![vec![1, -2, 3], vec![-1, 2, -3], vec![1, 2, 3]]);
        let mut state = DmmState::new(&f, 42);
        // Simulate different x_l values — clause 0 much higher than others
        state.x_l[0] = 100.0;
        state.x_l[1] = 1.0;
        state.x_l[2] = 2.0;
        // median of [100, 1, 2] sorted = [1, 2, 100], median = 2
        state.adjust_alpha_m();
        // Clause 0 (x_l=100 > median=2) → α_m should increase
        assert!(state.alpha_m[0] > 5.0, "alpha_m[0]={}", state.alpha_m[0]);
        // Clause 1 (x_l=1 ≤ median=2) → α_m should decrease
        assert!(state.alpha_m[1] < 5.0, "alpha_m[1]={}", state.alpha_m[1]);
    }
}
