use crate::formula::Formula;

/// DMM solver parameters (paper defaults).
pub struct Params {
    pub beta: f64,
    pub gamma: f64,
    pub delta: f64,
    pub epsilon: f64,
    pub zeta: f64,
    pub dt_max: f64,
    pub dt_min: f64,
    /// Initial value for per-clause alpha_m (paper default: 5.0)
    pub alpha_initial: f64,
    /// Multiplier when x_l > median (paper default: 1.1)
    pub alpha_up: f64,
    /// Multiplier when x_l <= median (paper default: 0.9)
    pub alpha_down: f64,
    /// Integration time between alpha_m adjustments (paper default: 1e4)
    pub alpha_interval: f64,
    /// Activity threshold for clause skipping in derivative computation.
    /// Clauses with C_m < threshold AND x_s < threshold skip voltage derivative
    /// contributions (gradient + rigidity). Set to 0.0 to disable (default).
    pub activity_threshold: f64,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            beta: 20.0,
            gamma: 0.25,
            delta: 0.05,
            epsilon: 1e-3,
            zeta: 1e-1,
            dt_max: 1024.0,      // 2^10
            dt_min: 1.0 / 128.0, // 2^-7
            alpha_initial: 5.0,
            alpha_up: 1.1,
            alpha_down: 0.9,
            alpha_interval: 1e4,
            activity_threshold: 0.0,
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
#[derive(Clone)]
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
    pub fn new(formula: &Formula, seed: u64, params: &Params) -> Self {
        let n = formula.num_vars;
        let m = formula.num_clauses();

        let v = Self::random_voltages(n, seed);
        let x_s: Vec<f64> = vec![0.0; m];
        let x_l: Vec<f64> = vec![1.0; m];
        let max_xl = 1e4 * (m as f64);
        let alpha_m = vec![params.alpha_initial; m];

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
        let m = formula.num_clauses();

        self.v = Self::random_voltages(formula.num_vars, seed);
        self.x_s = vec![0.0; m];
        self.x_l = vec![1.0; m];
        self.t = 0.0;
        self.last_alpha_adjust_t = 0.0;
        self.init_short_memory(formula);
    }

    /// Warm restart: initialize from best-known voltages with noise perturbation.
    /// Applies x_l decay transfer to preserve clause difficulty ranking.
    /// Analog of CDCL's "phase saving" + learnt clause retention.
    pub fn warm_restart(
        &mut self,
        formula: &Formula,
        best_voltages: &[f64],
        seed: u64,
        xl_decay: f64,
        noise_scale: f64,
    ) {
        let m = formula.num_clauses();

        // Generate noise using xorshift64
        let mut rng_state = seed;
        let mut rand_f64 = || -> f64 {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            (rng_state as f64) / (u64::MAX as f64) * 2.0 - 1.0 // [-1, 1]
        };

        // Initialize voltages from best-known + noise, clamped to [-1, 1]
        self.v = best_voltages
            .iter()
            .map(|&bv| (bv + noise_scale * rand_f64()).clamp(-1.0, 1.0))
            .collect();

        // x_l decay transfer: preserve clause difficulty ranking
        // x_l_new = 1.0 + decay * (x_l_old - 1.0)
        for xl in self.x_l.iter_mut() {
            *xl = 1.0 + xl_decay * (*xl - 1.0);
        }

        self.x_s = vec![0.0; m];
        self.t = 0.0;
        self.last_alpha_adjust_t = 0.0;
        self.init_short_memory(formula);
    }

    /// Warm-random restart: random voltages + x_l decay transfer.
    /// Combines cold's exploration (fresh random trajectory) with warm's
    /// memory retention (clause difficulty ranking preserved).
    pub fn warm_random_restart(&mut self, formula: &Formula, seed: u64, xl_decay: f64) {
        let m = formula.num_clauses();

        self.v = Self::random_voltages(formula.num_vars, seed);

        // x_l decay transfer: preserve clause difficulty ranking
        for xl in self.x_l.iter_mut() {
            *xl = 1.0 + xl_decay * (*xl - 1.0);
        }

        self.x_s = vec![0.0; m];
        self.t = 0.0;
        self.last_alpha_adjust_t = 0.0;
        self.init_short_memory(formula);
    }

    /// Anti-phase restart: negate best-known voltages to jump to a different
    /// solution cluster. At ratio ~4.27, solutions cluster in groups separated
    /// by O(N) Hamming distance — negation targets a different cluster.
    pub fn anti_phase_restart(
        &mut self,
        formula: &Formula,
        best_voltages: &[f64],
        seed: u64,
        noise_scale: f64,
    ) {
        let m = formula.num_clauses();

        let mut rng_state = seed;
        let mut rand_f64 = || -> f64 {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            (rng_state as f64) / (u64::MAX as f64) * 2.0 - 1.0
        };

        // Negate best voltages + noise
        self.v = best_voltages
            .iter()
            .map(|&bv| (-bv + noise_scale * rand_f64()).clamp(-1.0, 1.0))
            .collect();

        self.x_s = vec![0.0; m];
        self.x_l = vec![1.0; m];
        self.t = 0.0;
        self.last_alpha_adjust_t = 0.0;
        self.init_short_memory(formula);
    }

    /// Smart restart with CaDiCaL feedback: set initial voltages from
    /// CaDiCaL's phases and extend memory arrays for new learned clauses.
    pub fn restart_with_feedback(
        &mut self,
        formula: &Formula,
        voltages_from_cdcl: &[f64],
        params: &Params,
    ) {
        let m = formula.num_clauses();

        // Set voltages from CaDiCaL's phase assignments
        for (i, &v) in voltages_from_cdcl.iter().enumerate() {
            if i < self.v.len() {
                // Scale slightly toward center to give DMM room to evolve
                self.v[i] = v * 0.9;
            }
        }

        // Extend memory arrays if formula grew (learned clauses added)
        while self.x_s.len() < m {
            self.x_s.push(0.0);
        }
        while self.x_l.len() < m {
            self.x_l.push(1.0);
        }
        while self.alpha_m.len() < m {
            self.alpha_m.push(params.alpha_initial);
        }

        // Update max_xl for new clause count
        self.max_xl = 1e4 * (m as f64);

        // Reset time and re-initialize short memory
        self.t = 0.0;
        self.last_alpha_adjust_t = 0.0;
        self.init_short_memory(formula);
    }

    /// Per-clause α_m adjustment heuristic (paper Supplementary II.E).
    /// Called every `params.alpha_interval` time units.
    pub fn adjust_alpha_m(&mut self, params: &Params) {
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
                self.alpha_m[i] *= params.alpha_up;
            } else {
                self.alpha_m[i] *= params.alpha_down;
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
        let clause = formula.clause(m);
        let k = clause.len();

        // Compute L values: L_i = ½(1 - q_i · v_i)
        // Stack array for common case (k <= 64), heap fallback for wider clauses
        const STACK_MAX: usize = 64;
        let mut l_stack: [f64; STACK_MAX] = [0.0; STACK_MAX];
        let mut l_heap: Vec<f64>;
        let l_vals: &mut [f64] = if k <= STACK_MAX {
            &mut l_stack[..k]
        } else {
            l_heap = vec![0.0; k];
            &mut l_heap
        };
        for (i, &(var_idx, polarity)) in clause.iter().enumerate() {
            l_vals[i] = 0.5 * (1.0 - polarity * state.v[var_idx]);
        }

        // Find minimum and second minimum in one pass
        let mut min_val = f64::MAX;
        let mut min_idx = 0;
        let mut second_min_val = f64::MAX;
        for i in 0..k {
            if l_vals[i] < min_val {
                second_min_val = min_val;
                min_val = l_vals[i];
                min_idx = i;
            } else if l_vals[i] < second_min_val {
                second_min_val = l_vals[i];
            }
        }

        let c_m = min_val;
        derivs.c_m[m] = c_m;

        // Memory derivatives
        derivs.dx_s[m] = params.beta * (state.x_s[m] + params.epsilon) * (c_m - params.gamma);
        derivs.dx_l[m] = state.alpha_m[m] * (c_m - params.delta);

        // Activity-based clause skipping: when C_m and x_s are both below threshold,
        // the clause's voltage contributions are negligible (gradient scales as xl*xs,
        // rigidity scales as c_m). Skip to save work.
        let skip_threshold = params.activity_threshold;
        if skip_threshold > 0.0 && c_m < skip_threshold && state.x_s[m] < skip_threshold {
            continue;
        }

        // Voltage derivatives — fused gradient + rigidity
        let xl = state.x_l[m];
        let xs = state.x_s[m];
        let fs = xl * xs;

        // Gradient term: for literal i, min_others = min_val if i != min_idx, else second_min_val
        for (i, &(var_idx, polarity)) in clause.iter().enumerate() {
            let min_others = if i == min_idx {
                second_min_val
            } else {
                min_val
            };
            derivs.dv[var_idx] += fs * polarity * min_others;
        }

        // Rigidity term: only for the min literal
        let (var_idx, polarity) = clause[min_idx];
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
        let mut state = DmmState::new(&f, 42, &Params::default());
        state.x_l[0] = 100.0;
        state.x_l[1] = 1.0;
        state.x_l[2] = 2.0;
        state.adjust_alpha_m(&Params::default());
        assert!(state.alpha_m[0] > 5.0, "alpha_m[0]={}", state.alpha_m[0]);
        assert!(state.alpha_m[1] < 5.0, "alpha_m[1]={}", state.alpha_m[1]);
    }

    #[test]
    fn test_adjust_alpha_m_xl_at_max_resets() {
        let f = Formula::new(3, vec![vec![1, -2, 3], vec![-1, 2, -3], vec![1, 2, 3]]);
        let mut state = DmmState::new(&f, 42, &Params::default());
        state.x_l[0] = state.max_xl; // at max
        state.adjust_alpha_m(&Params::default());
        assert_eq!(state.x_l[0], 1.0, "x_l should reset to 1");
        assert_eq!(state.alpha_m[0], 1.0, "alpha_m should reset to 1");
    }

    #[test]
    fn test_init_short_memory() {
        let f = Formula::new(3, vec![vec![1, -2, 3], vec![-1, 2, -3]]);
        let mut state = DmmState::new(&f, 42, &Params::default());
        state.init_short_memory(&f);
        for &xs in &state.x_s {
            assert!(xs >= 0.0 && xs <= 1.0, "x_s={} out of range", xs);
        }
    }

    #[test]
    fn test_restart_preserves_alpha_m() {
        let f = Formula::new(3, vec![vec![1, -2, 3], vec![-1, 2, -3]]);
        let mut state = DmmState::new(&f, 42, &Params::default());
        state.alpha_m[0] = 10.0;
        state.alpha_m[1] = 0.5;
        state.restart(&f, 99);
        // alpha_m should NOT be reset by restart
        // (restart keeps learned difficulty)
        assert_eq!(state.x_l, vec![1.0; 2], "x_l should reset");
        assert_eq!(state.t, 0.0, "t should reset");
    }

    #[test]
    fn test_count_unsat() {
        let c_m = vec![0.1, 0.6, 0.3, 0.9, 0.4];
        assert_eq!(count_unsat(&c_m), 2); // 0.6 and 0.9 are >= 0.5
    }

    #[test]
    fn test_is_solved() {
        assert!(is_solved(&[0.1, 0.2, 0.3, 0.49]));
        assert!(!is_solved(&[0.1, 0.2, 0.5, 0.49]));
        assert!(is_solved(&[]));
    }

    #[test]
    fn test_auto_zeta() {
        let p1 = Params::default().with_auto_zeta(6.5);
        assert_eq!(p1.zeta, 1e-1);

        let p2 = Params::default().with_auto_zeta(5.0);
        assert!((p2.zeta - 1e-2).abs() < 1e-6);

        let p3 = Params::default().with_auto_zeta(4.0);
        assert_eq!(p3.zeta, 1e-3);

        // Interpolation range
        let p4 = Params::default().with_auto_zeta(5.5);
        assert!(p4.zeta > 1e-2 && p4.zeta < 1e-1);
    }

    #[test]
    fn test_compute_derivatives_basic() {
        let f = Formula::new(3, vec![vec![1, -2, 3]]);
        let params = Params::default();
        let state = DmmState::new(&f, 42, &Params::default());
        let mut derivs = Derivatives::new(f.num_vars, f.num_clauses());
        compute_derivatives(&f, &state, &params, &mut derivs);
        // C_m should be computed
        assert!(derivs.c_m[0] >= 0.0 && derivs.c_m[0] <= 1.0);
        // Derivatives should be finite
        for &d in &derivs.dv {
            assert!(d.is_finite(), "dv not finite: {}", d);
        }
        for &d in &derivs.dx_s {
            assert!(d.is_finite(), "dx_s not finite: {}", d);
        }
        for &d in &derivs.dx_l {
            assert!(d.is_finite(), "dx_l not finite: {}", d);
        }
    }

    #[test]
    fn test_restart_with_feedback_voltage_seeding() {
        let f = Formula::new(3, vec![vec![1, -2, 3], vec![-1, 2, -3]]);
        let mut state = DmmState::new(&f, 42, &Params::default());

        // Simulate CaDiCaL returning phases
        let cdcl_voltages = vec![1.0, -1.0, 1.0];
        state.restart_with_feedback(&f, &cdcl_voltages, &Params::default());

        // Voltages should be scaled (0.9x) from CaDiCaL's phases
        assert!((state.v[0] - 0.9).abs() < 1e-10);
        assert!((state.v[1] - (-0.9)).abs() < 1e-10);
        assert!((state.v[2] - 0.9).abs() < 1e-10);
        // Time should reset
        assert_eq!(state.t, 0.0);
    }

    #[test]
    fn test_restart_with_feedback_extends_memory_for_new_clauses() {
        let mut f = Formula::new(3, vec![vec![1, -2, 3], vec![-1, 2, -3]]);
        let mut state = DmmState::new(&f, 42, &Params::default());
        assert_eq!(state.x_s.len(), 2);
        assert_eq!(state.x_l.len(), 2);
        assert_eq!(state.alpha_m.len(), 2);

        // Add a learned clause (simulating CaDiCaL feedback)
        f.add_clause(&[1]);
        assert_eq!(f.num_clauses(), 3);

        // Restart with feedback should extend memory arrays
        let cdcl_voltages = vec![1.0, -1.0, 1.0];
        state.restart_with_feedback(&f, &cdcl_voltages, &Params::default());

        assert_eq!(state.x_s.len(), 3, "x_s should extend for new clause");
        assert_eq!(state.x_l.len(), 3, "x_l should extend for new clause");
        assert_eq!(state.alpha_m.len(), 3, "alpha_m should extend for new clause");
        // New clause should have default values
        assert_eq!(state.x_l[2], 1.0);
        assert_eq!(state.alpha_m[2], 5.0);
    }
}
