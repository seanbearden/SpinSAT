use crate::dmm::{clause_constraint, compute_derivatives, Derivatives, DmmState, Params};
use crate::formula::Formula;
use crate::sparse_deriv::SparseDerivEngine;

/// Selects which derivative computation engine to use.
pub enum DerivEngine {
    /// Champion: clause-by-clause loop (current implementation).
    Loop,
    /// Challenger: sparse matrix-vector products (MATLAB-style).
    Sparse(SparseDerivEngine),
}

impl DerivEngine {
    /// Compute derivatives using the selected engine.
    #[inline]
    pub fn compute(
        &mut self,
        formula: &Formula,
        state: &DmmState,
        params: &Params,
        derivs: &mut Derivatives,
    ) {
        match self {
            DerivEngine::Loop => compute_derivatives(formula, state, params, derivs),
            DerivEngine::Sparse(engine) => engine.compute(formula, state, params, derivs),
        }
    }

    /// Rebuild after formula changes (e.g., add_clause from CDCL).
    pub fn rebuild(&mut self, formula: &Formula) {
        if let DerivEngine::Sparse(_) = self {
            *self = DerivEngine::Sparse(SparseDerivEngine::from_formula(formula));
        }
    }
}

/// Integration method selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Method {
    Euler,
    Trapezoid,
    Rk4,
    /// Bogacki-Shampine 3(2) — 3rd-order with embedded 2nd-order error estimate.
    /// FSAL: 3 RHS evals/step (vs 4 for RK4). Use with PI step controller.
    Bs3,
    /// Strang splitting: half-step memories → full-step voltages (RK4) → half-step memories.
    /// 2nd-order splitting accuracy. Decouples memory stiffness from voltage integration.
    Strang,
}

impl Method {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "euler" => Some(Method::Euler),
            "trapezoid" | "trap" | "heun" => Some(Method::Trapezoid),
            "rk4" | "runge-kutta" | "rungekutta" => Some(Method::Rk4),
            "bs3" | "bogacki-shampine" => Some(Method::Bs3),
            "strang" | "split" => Some(Method::Strang),
            _ => None,
        }
    }
}

/// Compute adaptive time step from voltage derivatives.
#[inline]
fn adaptive_dt(dv: &[f64], params: &Params) -> f64 {
    let max_dv = dv.iter().map(|x| x.abs()).fold(0.0f64, f64::max);
    if max_dv > 0.0 {
        (params.dt_max.min(1.0 / max_dv)).max(params.dt_min)
    } else {
        params.dt_max
    }
}

/// Compute analytical x_s update for one clause.
/// Exact solution of dx_s/dt = β(x_s + ε)(C_m - γ) with C_m frozen over the step.
#[inline]
fn analytical_xs(x_s: f64, c_m: f64, dt: f64, beta: f64, epsilon: f64, gamma: f64) -> f64 {
    let exponent = beta * (c_m - gamma) * dt;
    ((x_s + epsilon) * exponent.exp() - epsilon).clamp(0.0, 1.0)
}

/// PI step size controller for embedded RK methods.
///
/// Uses the PI controller formula from Gustafsson et al.:
///   h_new = h_old * (err_n)^(-beta1) * (err_{n-1})^(beta2) * safety
///
/// Standard gains for BS3 (order p=3): beta1 = 0.6/(p+1) = 0.15, beta2 = 0.2/(p+1) = 0.05
pub struct PiController {
    /// Previous error estimate (for PI memory term).
    prev_err: f64,
    /// Current step size.
    pub dt: f64,
    /// Safety factor (typically 0.8-0.9).
    safety: f64,
    /// PI gain for current error.
    beta1: f64,
    /// PI gain for previous error.
    beta2: f64,
    /// Minimum step size.
    dt_min: f64,
    /// Maximum step size.
    dt_max: f64,
    /// Maximum step size growth factor per step.
    max_factor: f64,
    /// Minimum step size shrink factor per step.
    min_factor: f64,
    /// Error tolerance (relative).
    rtol: f64,
}

impl PiController {
    /// Create a new PI controller with settings tuned for BS3 equilibrium-seeking.
    pub fn new(dt_min: f64, dt_max: f64) -> Self {
        let p = 3.0; // BS3 order
        PiController {
            prev_err: 1.0,
            dt: 1.0, // start at dt=1.0 — let the controller find the right scale
            safety: 0.9,
            beta1: 0.6 / (p + 1.0), // 0.15
            beta2: 0.2 / (p + 1.0), // 0.05
            dt_min,
            dt_max,
            max_factor: 5.0,
            min_factor: 0.2,
            rtol: 0.5, // very relaxed — we seek equilibria, not accurate trajectories
        }
    }

    /// Propose next step size based on error estimate.
    /// Returns (accepted, new_dt). If rejected, caller should retry with new_dt.
    pub fn propose(&mut self, err_norm: f64) -> (bool, f64) {
        let err = err_norm / self.rtol;
        if err <= 0.0 {
            // Zero error — take maximum step
            let new_dt = (self.dt * self.max_factor).min(self.dt_max);
            self.prev_err = 1.0;
            self.dt = new_dt;
            return (true, new_dt);
        }

        // PI controller: factor = err^(-beta1) * prev_err^(beta2)
        let factor = err.powf(-self.beta1) * self.prev_err.powf(self.beta2);
        let factor = (factor * self.safety).clamp(self.min_factor, self.max_factor);
        let new_dt = (self.dt * factor).clamp(self.dt_min, self.dt_max);

        if err <= 1.0 {
            // Step accepted
            self.prev_err = err;
            self.dt = new_dt;
            (true, new_dt)
        } else {
            // Step rejected — shrink but don't update prev_err
            self.dt = new_dt;
            (false, new_dt)
        }
    }

    /// Reset controller state (e.g., after a restart).
    pub fn reset(&mut self) {
        self.prev_err = 1.0;
        self.dt = 1.0;
    }
}

/// FSAL (First Same As Last) state for BS3.
/// Stores the derivatives from the last accepted step's final stage,
/// which become the first stage of the next step.
pub struct FsalState {
    /// Whether we have valid FSAL data from a previous step.
    pub valid: bool,
    /// Cached derivatives from the previous step's final stage.
    pub derivs: Derivatives,
}

impl FsalState {
    pub fn new(num_vars: usize, num_clauses: usize) -> Self {
        FsalState {
            valid: false,
            derivs: Derivatives::new(num_vars, num_clauses),
        }
    }

    /// Invalidate FSAL cache (e.g., after restart or state modification).
    pub fn invalidate(&mut self) {
        self.valid = false;
    }
}

/// Post-step bookkeeping: track time and check α_m adjustment.
fn post_step(state: &mut DmmState, params: &Params, dt: f64) {
    state.t += dt;
    if state.t - state.last_alpha_adjust_t >= params.alpha_interval {
        state.adjust_alpha_m(params);
    }
}

/// Scratch buffers for multi-stage methods (Trapezoid, RK4).
pub struct ScratchBuffers {
    pub tmp_state: Option<DmmState>,
    pub d2: Option<Derivatives>,
    pub d3: Option<Derivatives>,
    pub d4: Option<Derivatives>,
}

impl ScratchBuffers {
    pub fn new(formula: &Formula, base_state: &DmmState) -> Self {
        let n = formula.num_vars;
        let m = formula.num_clauses();

        let tmp = DmmState {
            v: vec![0.0; n],
            x_s: vec![0.0; m],
            x_l: vec![1.0; m],
            max_xl: base_state.max_xl,
            alpha_m: base_state.alpha_m.clone(),
            t: 0.0,
            last_alpha_adjust_t: 0.0,
        };

        ScratchBuffers {
            tmp_state: Some(tmp),
            d2: Some(Derivatives::new(n, m)),
            d3: Some(Derivatives::new(n, m)),
            d4: Some(Derivatives::new(n, m)),
        }
    }

    pub fn empty() -> Self {
        ScratchBuffers {
            tmp_state: None,
            d2: None,
            d3: None,
            d4: None,
        }
    }
}

/// Set tmp_state = base_state + dt * derivatives, with clamping.
/// Uses analytical x_s update (exact solution with C_m frozen) instead of Euler step.
fn set_tmp_state(
    tmp: &mut DmmState,
    base: &DmmState,
    dv: &[f64],
    c_m: &[f64],
    dx_l: &[f64],
    dt: f64,
    params: &Params,
) {
    for i in 0..base.v.len() {
        tmp.v[i] = (base.v[i] + dt * dv[i]).clamp(-1.0, 1.0);
    }
    for i in 0..base.x_s.len() {
        tmp.x_s[i] =
            analytical_xs(base.x_s[i], c_m[i], dt, params.beta, params.epsilon, params.gamma);
    }
    for i in 0..base.x_l.len() {
        tmp.x_l[i] = (base.x_l[i] + dt * dx_l[i]).clamp(1.0, base.max_xl);
    }
    // Copy alpha_m from base (it doesn't change during intermediate steps)
    tmp.alpha_m.clear();
    tmp.alpha_m.extend_from_slice(&base.alpha_m);
}

/// Perform one integration step using the selected method.
pub fn integration_step(
    method: Method,
    formula: &Formula,
    state: &mut DmmState,
    params: &Params,
    derivs: &mut Derivatives,
    scratch: &mut ScratchBuffers,
    dt: f64,
) -> f64 {
    match method {
        Method::Euler => euler_step(formula, state, params, derivs, dt),
        Method::Trapezoid => trapezoid_step(formula, state, params, derivs, scratch, dt),
        Method::Rk4 => rk4_step(formula, state, params, derivs, scratch, dt),
        Method::Bs3 => {
            // BS3 without FSAL/PI falls back to a single step with heuristic dt.
            // For proper BS3 usage, call bs3_step_with_pi directly.
            bs3_step(formula, state, params, derivs, scratch, dt)
        }
        Method::Strang => {
            strang_step(formula, state, params, derivs, scratch, dt)
        }
    }
}

/// Perform one integration step using a specific derivative engine.
pub fn integration_step_with_engine(
    method: Method,
    formula: &Formula,
    state: &mut DmmState,
    params: &Params,
    derivs: &mut Derivatives,
    scratch: &mut ScratchBuffers,
    dt: f64,
    engine: &mut DerivEngine,
) -> f64 {
    match method {
        Method::Euler => euler_step_with_engine(formula, state, params, derivs, dt, engine),
        Method::Trapezoid => {
            trapezoid_step_with_engine(formula, state, params, derivs, scratch, dt, engine)
        }
        Method::Rk4 => {
            rk4_step_with_engine(formula, state, params, derivs, scratch, dt, engine)
        }
        Method::Bs3 => {
            bs3_step_with_engine(formula, state, params, derivs, scratch, dt, engine)
        }
        Method::Strang => {
            strang_step_with_engine(formula, state, params, derivs, scratch, dt, engine)
        }
    }
}

/// Perform one BS3 step with FSAL and PI step control.
/// This is the preferred entry point for BS3 — handles step rejection, FSAL caching,
/// and returns the error norm for diagnostics.
/// Returns (actual_dt, err_norm).
pub fn bs3_step_with_pi(
    formula: &Formula,
    state: &mut DmmState,
    params: &Params,
    derivs: &mut Derivatives,
    scratch: &mut ScratchBuffers,
    fsal: &mut FsalState,
    pi: &mut PiController,
    engine: &mut DerivEngine,
) -> (f64, f64) {
    let m = formula.num_clauses();
    let n = formula.num_vars;

    // Stage 1: use FSAL cache or compute fresh
    if fsal.valid {
        // Copy cached FSAL derivs into derivs
        derivs.dv.copy_from_slice(&fsal.derivs.dv);
        derivs.dx_s.copy_from_slice(&fsal.derivs.dx_s);
        derivs.dx_l.copy_from_slice(&fsal.derivs.dx_l);
        derivs.c_m.copy_from_slice(&fsal.derivs.c_m);
    } else {
        engine.compute(formula, state, params, derivs);
    }

    let dt = pi.dt;
    let tmp = scratch.tmp_state.as_mut().unwrap();

    // Stage 2: k2 at y + dt/2 * k1
    let half_dt = dt * 0.5;
    set_tmp_state(tmp, state, &derivs.dv, &derivs.c_m, &derivs.dx_l, half_dt, params);
    let d2 = scratch.d2.as_mut().unwrap();
    engine.compute(formula, tmp, params, d2);

    // Stage 3: k3 at y + 3/4*dt * k2
    let three_quarter_dt = dt * 0.75;
    set_tmp_v_only(tmp, state, &d2.dv, three_quarter_dt);
    for i in 0..m {
        tmp.x_s[i] =
            analytical_xs(state.x_s[i], d2.c_m[i], three_quarter_dt, params.beta, params.epsilon, params.gamma);
    }
    for i in 0..m {
        tmp.x_l[i] = (state.x_l[i] + three_quarter_dt * d2.dx_l[i]).clamp(1.0, state.max_xl);
    }
    let d3 = scratch.d3.as_mut().unwrap();
    engine.compute(formula, tmp, params, d3);

    // 3rd-order solution: y_new = y + dt * (2/9*k1 + 1/3*k2 + 4/9*k3)
    // Compute error estimate: err = y_3rd - y_2nd
    //   = dt * ((2/9 - 7/24)*k1 + (1/3 - 1/4)*k2 + (4/9 - 1/3)*k3 - 1/8*k4)
    //   = dt * (-1/72*k1 + 1/12*k2 + 1/9*k3 - 1/8*k4)
    // But we need k4 first, which requires the 3rd-order state.

    // Apply 3rd-order update to get new state
    let c1 = 2.0 / 9.0;
    let c2 = 1.0 / 3.0;
    let c3 = 4.0 / 9.0;

    // Save old state for potential rejection
    let v_old: Vec<f64> = state.v.clone();
    let xs_old: Vec<f64> = state.x_s.clone();
    let xl_old: Vec<f64> = state.x_l.clone();
    let t_old = state.t;

    for i in 0..m {
        let dx = c1 * derivs.dx_l[i] + c2 * d2.dx_l[i] + c3 * d3.dx_l[i];
        state.x_l[i] = (state.x_l[i] + dt * dx).clamp(1.0, state.max_xl);
    }
    for i in 0..m {
        let avg_cm = c1 * derivs.c_m[i] + c2 * d2.c_m[i] + c3 * d3.c_m[i];
        state.x_s[i] = analytical_xs(
            state.x_s[i], avg_cm, dt, params.beta, params.epsilon, params.gamma,
        );
    }
    for i in 0..n {
        let dx = c1 * derivs.dv[i] + c2 * d2.dv[i] + c3 * d3.dv[i];
        state.v[i] = (state.v[i] + dt * dx).clamp(-1.0, 1.0);
    }

    // Stage 4 (FSAL): k4 at the new state
    engine.compute(formula, state, params, &mut fsal.derivs);

    // Error estimate: diff between 3rd and 2nd order solutions (voltage only)
    // err = dt * (-1/72*k1 + 1/12*k2 + 1/9*k3 - 1/8*k4)
    let e1 = -1.0 / 72.0;
    let e2 = 1.0 / 12.0;
    let e3 = 1.0 / 9.0;
    let e4 = -1.0 / 8.0;

    let mut err_max: f64 = 0.0;
    for i in 0..n {
        let ei = dt * (e1 * derivs.dv[i] + e2 * d2.dv[i] + e3 * d3.dv[i] + e4 * fsal.derivs.dv[i]);
        // Scale by max(|v_old|, |v_new|, threshold) for relative error
        let scale = state.v[i].abs().max(v_old[i].abs()).max(0.1);
        let rel_err = (ei / scale).abs();
        if rel_err > err_max {
            err_max = rel_err;
        }
    }

    let (accepted, _new_dt) = pi.propose(err_max);

    if accepted {
        // Update c_m for solution checking
        derivs.c_m.copy_from_slice(&fsal.derivs.c_m);
        fsal.valid = true;
        post_step(state, params, dt);
        (dt, err_max)
    } else {
        // Reject: restore old state
        state.v.copy_from_slice(&v_old);
        state.x_s.copy_from_slice(&xs_old);
        state.x_l.copy_from_slice(&xl_old);
        state.t = t_old;
        fsal.valid = false;
        // Return negative dt to signal rejection to caller
        (-dt, err_max)
    }
}

/// Helper: set only the voltage part of tmp_state (for BS3 stage 3 with non-standard coefficients).
#[inline]
fn set_tmp_v_only(tmp: &mut DmmState, base: &DmmState, dv: &[f64], dt: f64) {
    for i in 0..base.v.len() {
        tmp.v[i] = (base.v[i] + dt * dv[i]).clamp(-1.0, 1.0);
    }
}

/// Forward Euler integration step.
pub fn euler_step(
    formula: &Formula,
    state: &mut DmmState,
    params: &Params,
    derivs: &mut Derivatives,
    dt: f64,
) -> f64 {
    compute_derivatives(formula, state, params, derivs);

    let actual_dt = if dt < 0.0 {
        adaptive_dt(&derivs.dv, params)
    } else {
        dt
    };

    let m = formula.num_clauses();
    let n = formula.num_vars;

    for i in 0..m {
        state.x_l[i] = (state.x_l[i] + derivs.dx_l[i] * actual_dt).clamp(1.0, state.max_xl);
    }
    for i in 0..m {
        state.x_s[i] = analytical_xs(
            state.x_s[i],
            derivs.c_m[i],
            actual_dt,
            params.beta,
            params.epsilon,
            params.gamma,
        );
    }
    for i in 0..n {
        state.v[i] = (state.v[i] + derivs.dv[i] * actual_dt).clamp(-1.0, 1.0);
    }

    post_step(state, params, actual_dt);
    actual_dt
}

/// Trapezoid (Heun's method) — 2 derivative evaluations per step.
fn trapezoid_step(
    formula: &Formula,
    state: &mut DmmState,
    params: &Params,
    derivs: &mut Derivatives,
    scratch: &mut ScratchBuffers,
    dt: f64,
) -> f64 {
    let m = formula.num_clauses();
    let n = formula.num_vars;

    // Stage 1: k1 at current state
    compute_derivatives(formula, state, params, derivs);

    let actual_dt = if dt < 0.0 {
        adaptive_dt(&derivs.dv, params)
    } else {
        dt
    };

    // Stage 2: k2 at Euler-predicted state
    let tmp = scratch.tmp_state.as_mut().unwrap();
    set_tmp_state(
        tmp, state, &derivs.dv, &derivs.c_m, &derivs.dx_l, actual_dt, params,
    );
    let d2 = scratch.d2.as_mut().unwrap();
    compute_derivatives(formula, tmp, params, d2);

    // Update: y += dt/2 * (k1 + k2)
    let half_dt = actual_dt * 0.5;
    for i in 0..m {
        state.x_l[i] =
            (state.x_l[i] + half_dt * (derivs.dx_l[i] + d2.dx_l[i])).clamp(1.0, state.max_xl);
    }
    for i in 0..m {
        let avg_cm = 0.5 * (derivs.c_m[i] + d2.c_m[i]);
        state.x_s[i] = analytical_xs(
            state.x_s[i],
            avg_cm,
            actual_dt,
            params.beta,
            params.epsilon,
            params.gamma,
        );
    }
    for i in 0..n {
        state.v[i] = (state.v[i] + half_dt * (derivs.dv[i] + d2.dv[i])).clamp(-1.0, 1.0);
    }

    // Use stage 2 c_m for solution checking
    derivs.c_m.copy_from_slice(&d2.c_m);

    post_step(state, params, actual_dt);
    actual_dt
}

/// RK4 (classical Runge-Kutta) — 4 derivative evaluations per step.
fn rk4_step(
    formula: &Formula,
    state: &mut DmmState,
    params: &Params,
    derivs: &mut Derivatives,
    scratch: &mut ScratchBuffers,
    dt: f64,
) -> f64 {
    let m = formula.num_clauses();
    let n = formula.num_vars;

    // Stage 1: k1 at current state
    compute_derivatives(formula, state, params, derivs);

    let actual_dt = if dt < 0.0 {
        adaptive_dt(&derivs.dv, params)
    } else {
        dt
    };

    let half_dt = actual_dt * 0.5;
    let tmp = scratch.tmp_state.as_mut().unwrap();

    // Stage 2: k2 at y + dt/2 * k1
    set_tmp_state(tmp, state, &derivs.dv, &derivs.c_m, &derivs.dx_l, half_dt, params);
    let d2 = scratch.d2.as_mut().unwrap();
    compute_derivatives(formula, tmp, params, d2);

    // Stage 3: k3 at y + dt/2 * k2
    set_tmp_state(tmp, state, &d2.dv, &d2.c_m, &d2.dx_l, half_dt, params);
    let d3 = scratch.d3.as_mut().unwrap();
    compute_derivatives(formula, tmp, params, d3);

    // Stage 4: k4 at y + dt * k3
    set_tmp_state(tmp, state, &d3.dv, &d3.c_m, &d3.dx_l, actual_dt, params);
    let d4 = scratch.d4.as_mut().unwrap();
    compute_derivatives(formula, tmp, params, d4);

    // Update: y += dt/6 * (k1 + 2*k2 + 2*k3 + k4)
    let dt_sixth = actual_dt / 6.0;
    for i in 0..m {
        let dx = derivs.dx_l[i] + 2.0 * d2.dx_l[i] + 2.0 * d3.dx_l[i] + d4.dx_l[i];
        state.x_l[i] = (state.x_l[i] + dt_sixth * dx).clamp(1.0, state.max_xl);
    }
    for i in 0..m {
        let avg_cm = (derivs.c_m[i] + 2.0 * d2.c_m[i] + 2.0 * d3.c_m[i] + d4.c_m[i]) / 6.0;
        state.x_s[i] = analytical_xs(
            state.x_s[i],
            avg_cm,
            actual_dt,
            params.beta,
            params.epsilon,
            params.gamma,
        );
    }
    for i in 0..n {
        let dx = derivs.dv[i] + 2.0 * d2.dv[i] + 2.0 * d3.dv[i] + d4.dv[i];
        state.v[i] = (state.v[i] + dt_sixth * dx).clamp(-1.0, 1.0);
    }

    derivs.c_m.copy_from_slice(&d4.c_m);

    post_step(state, params, actual_dt);
    actual_dt
}

// --- Engine-aware variants (same logic, pluggable derivative computation) ---

fn euler_step_with_engine(
    formula: &Formula,
    state: &mut DmmState,
    params: &Params,
    derivs: &mut Derivatives,
    dt: f64,
    engine: &mut DerivEngine,
) -> f64 {
    engine.compute(formula, state, params, derivs);

    let actual_dt = if dt < 0.0 {
        adaptive_dt(&derivs.dv, params)
    } else {
        dt
    };

    let m = formula.num_clauses();
    let n = formula.num_vars;
    for i in 0..m {
        state.x_l[i] = (state.x_l[i] + derivs.dx_l[i] * actual_dt).clamp(1.0, state.max_xl);
    }
    for i in 0..m {
        state.x_s[i] = analytical_xs(
            state.x_s[i],
            derivs.c_m[i],
            actual_dt,
            params.beta,
            params.epsilon,
            params.gamma,
        );
    }
    for i in 0..n {
        state.v[i] = (state.v[i] + derivs.dv[i] * actual_dt).clamp(-1.0, 1.0);
    }
    post_step(state, params, actual_dt);
    actual_dt
}

fn trapezoid_step_with_engine(
    formula: &Formula,
    state: &mut DmmState,
    params: &Params,
    derivs: &mut Derivatives,
    scratch: &mut ScratchBuffers,
    dt: f64,
    engine: &mut DerivEngine,
) -> f64 {
    let m = formula.num_clauses();
    let n = formula.num_vars;

    engine.compute(formula, state, params, derivs);
    let actual_dt = if dt < 0.0 {
        adaptive_dt(&derivs.dv, params)
    } else {
        dt
    };

    let tmp = scratch.tmp_state.as_mut().unwrap();
    set_tmp_state(tmp, state, &derivs.dv, &derivs.c_m, &derivs.dx_l, actual_dt, params);
    let d2 = scratch.d2.as_mut().unwrap();
    engine.compute(formula, tmp, params, d2);

    let half_dt = actual_dt * 0.5;
    for i in 0..m {
        state.x_l[i] =
            (state.x_l[i] + half_dt * (derivs.dx_l[i] + d2.dx_l[i])).clamp(1.0, state.max_xl);
    }
    for i in 0..m {
        let avg_cm = 0.5 * (derivs.c_m[i] + d2.c_m[i]);
        state.x_s[i] = analytical_xs(
            state.x_s[i],
            avg_cm,
            actual_dt,
            params.beta,
            params.epsilon,
            params.gamma,
        );
    }
    for i in 0..n {
        state.v[i] = (state.v[i] + half_dt * (derivs.dv[i] + d2.dv[i])).clamp(-1.0, 1.0);
    }
    derivs.c_m.copy_from_slice(&d2.c_m);
    post_step(state, params, actual_dt);
    actual_dt
}

fn rk4_step_with_engine(
    formula: &Formula,
    state: &mut DmmState,
    params: &Params,
    derivs: &mut Derivatives,
    scratch: &mut ScratchBuffers,
    dt: f64,
    engine: &mut DerivEngine,
) -> f64 {
    let m = formula.num_clauses();
    let n = formula.num_vars;

    engine.compute(formula, state, params, derivs);
    let actual_dt = if dt < 0.0 {
        adaptive_dt(&derivs.dv, params)
    } else {
        dt
    };

    let half_dt = actual_dt * 0.5;
    let tmp = scratch.tmp_state.as_mut().unwrap();

    set_tmp_state(tmp, state, &derivs.dv, &derivs.c_m, &derivs.dx_l, half_dt, params);
    let d2 = scratch.d2.as_mut().unwrap();
    engine.compute(formula, tmp, params, d2);

    set_tmp_state(tmp, state, &d2.dv, &d2.c_m, &d2.dx_l, half_dt, params);
    let d3 = scratch.d3.as_mut().unwrap();
    engine.compute(formula, tmp, params, d3);

    set_tmp_state(tmp, state, &d3.dv, &d3.c_m, &d3.dx_l, actual_dt, params);
    let d4 = scratch.d4.as_mut().unwrap();
    engine.compute(formula, tmp, params, d4);

    let dt_sixth = actual_dt / 6.0;
    for i in 0..m {
        let dx = derivs.dx_l[i] + 2.0 * d2.dx_l[i] + 2.0 * d3.dx_l[i] + d4.dx_l[i];
        state.x_l[i] = (state.x_l[i] + dt_sixth * dx).clamp(1.0, state.max_xl);
    }
    for i in 0..m {
        let avg_cm = (derivs.c_m[i] + 2.0 * d2.c_m[i] + 2.0 * d3.c_m[i] + d4.c_m[i]) / 6.0;
        state.x_s[i] = analytical_xs(
            state.x_s[i],
            avg_cm,
            actual_dt,
            params.beta,
            params.epsilon,
            params.gamma,
        );
    }
    for i in 0..n {
        let dx = derivs.dv[i] + 2.0 * d2.dv[i] + 2.0 * d3.dv[i] + d4.dv[i];
        state.v[i] = (state.v[i] + dt_sixth * dx).clamp(-1.0, 1.0);
    }
    derivs.c_m.copy_from_slice(&d4.c_m);
    post_step(state, params, actual_dt);
    actual_dt
}

/// BS3 single step without FSAL/PI (fallback for integration_step interface).
fn bs3_step(
    formula: &Formula,
    state: &mut DmmState,
    params: &Params,
    derivs: &mut Derivatives,
    scratch: &mut ScratchBuffers,
    dt: f64,
) -> f64 {
    let m = formula.num_clauses();
    let n = formula.num_vars;

    compute_derivatives(formula, state, params, derivs);
    let actual_dt = if dt < 0.0 {
        adaptive_dt(&derivs.dv, params)
    } else {
        dt
    };

    let tmp = scratch.tmp_state.as_mut().unwrap();

    // Stage 2: k2 at y + dt/2 * k1
    let half_dt = actual_dt * 0.5;
    set_tmp_state(tmp, state, &derivs.dv, &derivs.c_m, &derivs.dx_l, half_dt, params);
    let d2 = scratch.d2.as_mut().unwrap();
    compute_derivatives(formula, tmp, params, d2);

    // Stage 3: k3 at y + 3/4*dt * k2
    let three_quarter_dt = actual_dt * 0.75;
    set_tmp_v_only(tmp, state, &d2.dv, three_quarter_dt);
    for i in 0..m {
        tmp.x_s[i] =
            analytical_xs(state.x_s[i], d2.c_m[i], three_quarter_dt, params.beta, params.epsilon, params.gamma);
    }
    for i in 0..m {
        tmp.x_l[i] = (state.x_l[i] + three_quarter_dt * d2.dx_l[i]).clamp(1.0, state.max_xl);
    }
    let d3 = scratch.d3.as_mut().unwrap();
    compute_derivatives(formula, tmp, params, d3);

    // 3rd-order update: y += dt * (2/9*k1 + 1/3*k2 + 4/9*k3)
    let c1 = 2.0 / 9.0;
    let c2 = 1.0 / 3.0;
    let c3 = 4.0 / 9.0;

    for i in 0..m {
        let dx = c1 * derivs.dx_l[i] + c2 * d2.dx_l[i] + c3 * d3.dx_l[i];
        state.x_l[i] = (state.x_l[i] + actual_dt * dx).clamp(1.0, state.max_xl);
    }
    for i in 0..m {
        let avg_cm = c1 * derivs.c_m[i] + c2 * d2.c_m[i] + c3 * d3.c_m[i];
        state.x_s[i] = analytical_xs(
            state.x_s[i], avg_cm, actual_dt, params.beta, params.epsilon, params.gamma,
        );
    }
    for i in 0..n {
        let dx = c1 * derivs.dv[i] + c2 * d2.dv[i] + c3 * d3.dv[i];
        state.v[i] = (state.v[i] + actual_dt * dx).clamp(-1.0, 1.0);
    }

    derivs.c_m.copy_from_slice(&d3.c_m);
    post_step(state, params, actual_dt);
    actual_dt
}

/// BS3 single step with engine (fallback for integration_step_with_engine interface).
fn bs3_step_with_engine(
    formula: &Formula,
    state: &mut DmmState,
    params: &Params,
    derivs: &mut Derivatives,
    scratch: &mut ScratchBuffers,
    dt: f64,
    engine: &mut DerivEngine,
) -> f64 {
    let m = formula.num_clauses();
    let n = formula.num_vars;

    engine.compute(formula, state, params, derivs);
    let actual_dt = if dt < 0.0 {
        adaptive_dt(&derivs.dv, params)
    } else {
        dt
    };

    let tmp = scratch.tmp_state.as_mut().unwrap();

    let half_dt = actual_dt * 0.5;
    set_tmp_state(tmp, state, &derivs.dv, &derivs.c_m, &derivs.dx_l, half_dt, params);
    let d2 = scratch.d2.as_mut().unwrap();
    engine.compute(formula, tmp, params, d2);

    let three_quarter_dt = actual_dt * 0.75;
    set_tmp_v_only(tmp, state, &d2.dv, three_quarter_dt);
    for i in 0..m {
        tmp.x_s[i] =
            analytical_xs(state.x_s[i], d2.c_m[i], three_quarter_dt, params.beta, params.epsilon, params.gamma);
    }
    for i in 0..m {
        tmp.x_l[i] = (state.x_l[i] + three_quarter_dt * d2.dx_l[i]).clamp(1.0, state.max_xl);
    }
    let d3 = scratch.d3.as_mut().unwrap();
    engine.compute(formula, tmp, params, d3);

    let c1 = 2.0 / 9.0;
    let c2 = 1.0 / 3.0;
    let c3 = 4.0 / 9.0;

    for i in 0..m {
        let dx = c1 * derivs.dx_l[i] + c2 * d2.dx_l[i] + c3 * d3.dx_l[i];
        state.x_l[i] = (state.x_l[i] + actual_dt * dx).clamp(1.0, state.max_xl);
    }
    for i in 0..m {
        let avg_cm = c1 * derivs.c_m[i] + c2 * d2.c_m[i] + c3 * d3.c_m[i];
        state.x_s[i] = analytical_xs(
            state.x_s[i], avg_cm, actual_dt, params.beta, params.epsilon, params.gamma,
        );
    }
    for i in 0..n {
        let dx = c1 * derivs.dv[i] + c2 * d2.dv[i] + c3 * d3.dv[i];
        state.v[i] = (state.v[i] + actual_dt * dx).clamp(-1.0, 1.0);
    }

    derivs.c_m.copy_from_slice(&d3.c_m);
    post_step(state, params, actual_dt);
    actual_dt
}

/// Compute all C_m values from current voltages (batch version of clause_constraint).
#[inline]
fn compute_all_cm(formula: &Formula, v: &[f64], c_m: &mut [f64]) {
    for m in 0..formula.num_clauses() {
        c_m[m] = clause_constraint(formula, m, v);
    }
}

/// Update memories (x_s analytically, x_l linearly) for a half-step with frozen C_m.
#[inline]
fn memory_half_step(state: &mut DmmState, c_m: &[f64], half_dt: f64, params: &Params) {
    let m = c_m.len();
    for i in 0..m {
        state.x_s[i] = analytical_xs(
            state.x_s[i], c_m[i], half_dt, params.beta, params.epsilon, params.gamma,
        );
    }
    for i in 0..m {
        let dx_l = state.alpha_m[i] * (c_m[i] - params.delta);
        state.x_l[i] = (state.x_l[i] + dx_l * half_dt).clamp(1.0, state.max_xl);
    }
}

/// Strang splitting step: half-memory → full-voltage(RK4) → half-memory.
///
/// 1. Compute C_m from current v
/// 2. Half-step: update x_s (analytical) and x_l (linear) with frozen C_m
/// 3. Full-step: compute full derivatives with updated memories, advance v using RK4
/// 4. Recompute C_m from new v
/// 5. Half-step: update x_s and x_l again with new C_m
fn strang_step(
    formula: &Formula,
    state: &mut DmmState,
    params: &Params,
    derivs: &mut Derivatives,
    scratch: &mut ScratchBuffers,
    dt: f64,
) -> f64 {
    let n = formula.num_vars;

    // Step 1: Compute C_m from current state
    compute_all_cm(formula, &state.v, &mut derivs.c_m);

    // Determine adaptive dt from a quick derivative evaluation (voltage part only matters)
    compute_derivatives(formula, state, params, derivs);
    let actual_dt = if dt < 0.0 {
        adaptive_dt(&derivs.dv, params)
    } else {
        dt
    };
    let half_dt = actual_dt * 0.5;

    // Step 2: Half-step memories with frozen C_m
    memory_half_step(state, &derivs.c_m, half_dt, params);

    // Step 3: Full-step voltages using RK4 with updated memories
    // Recompute derivatives with updated x_s, x_l
    compute_derivatives(formula, state, params, derivs);
    // We already have k1 in derivs. Now do RK4 stages for voltage only.
    let tmp = scratch.tmp_state.as_mut().unwrap();

    // Stage 2: k2 at y + dt/2 * k1 (voltage only — memories are already updated)
    set_tmp_state(tmp, state, &derivs.dv, &derivs.c_m, &derivs.dx_l, half_dt, params);
    let d2 = scratch.d2.as_mut().unwrap();
    compute_derivatives(formula, tmp, params, d2);

    // Stage 3: k3 at y + dt/2 * k2
    set_tmp_state(tmp, state, &d2.dv, &d2.c_m, &d2.dx_l, half_dt, params);
    let d3 = scratch.d3.as_mut().unwrap();
    compute_derivatives(formula, tmp, params, d3);

    // Stage 4: k4 at y + dt * k3
    set_tmp_state(tmp, state, &d3.dv, &d3.c_m, &d3.dx_l, actual_dt, params);
    let d4 = scratch.d4.as_mut().unwrap();
    compute_derivatives(formula, tmp, params, d4);

    // RK4 voltage update only
    let dt_sixth = actual_dt / 6.0;
    for i in 0..n {
        let dx = derivs.dv[i] + 2.0 * d2.dv[i] + 2.0 * d3.dv[i] + d4.dv[i];
        state.v[i] = (state.v[i] + dt_sixth * dx).clamp(-1.0, 1.0);
    }

    // Step 4: Recompute C_m from new voltages
    compute_all_cm(formula, &state.v, &mut derivs.c_m);

    // Step 5: Half-step memories with new C_m
    memory_half_step(state, &derivs.c_m, half_dt, params);

    post_step(state, params, actual_dt);
    actual_dt
}

/// Strang splitting step with pluggable derivative engine.
fn strang_step_with_engine(
    formula: &Formula,
    state: &mut DmmState,
    params: &Params,
    derivs: &mut Derivatives,
    scratch: &mut ScratchBuffers,
    dt: f64,
    engine: &mut DerivEngine,
) -> f64 {
    let n = formula.num_vars;

    // Step 1: Compute C_m and derivatives
    engine.compute(formula, state, params, derivs);
    let actual_dt = if dt < 0.0 {
        adaptive_dt(&derivs.dv, params)
    } else {
        dt
    };
    let half_dt = actual_dt * 0.5;

    // Step 2: Half-step memories with frozen C_m
    memory_half_step(state, &derivs.c_m, half_dt, params);

    // Step 3: Full-step voltages using RK4 with updated memories
    engine.compute(formula, state, params, derivs);
    let tmp = scratch.tmp_state.as_mut().unwrap();

    set_tmp_state(tmp, state, &derivs.dv, &derivs.c_m, &derivs.dx_l, half_dt, params);
    let d2 = scratch.d2.as_mut().unwrap();
    engine.compute(formula, tmp, params, d2);

    set_tmp_state(tmp, state, &d2.dv, &d2.c_m, &d2.dx_l, half_dt, params);
    let d3 = scratch.d3.as_mut().unwrap();
    engine.compute(formula, tmp, params, d3);

    set_tmp_state(tmp, state, &d3.dv, &d3.c_m, &d3.dx_l, actual_dt, params);
    let d4 = scratch.d4.as_mut().unwrap();
    engine.compute(formula, tmp, params, d4);

    let dt_sixth = actual_dt / 6.0;
    for i in 0..n {
        let dx = derivs.dv[i] + 2.0 * d2.dv[i] + 2.0 * d3.dv[i] + d4.dv[i];
        state.v[i] = (state.v[i] + dt_sixth * dx).clamp(-1.0, 1.0);
    }

    // Step 4: Recompute C_m from new voltages
    compute_all_cm(formula, &state.v, &mut derivs.c_m);

    // Step 5: Half-step memories with new C_m
    memory_half_step(state, &derivs.c_m, half_dt, params);

    post_step(state, params, actual_dt);
    actual_dt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::Formula;

    fn test_formula() -> Formula {
        Formula::new(2, vec![vec![1, 2], vec![-1, 2]])
    }

    fn assert_bounds(state: &DmmState) {
        for &v in &state.v {
            assert!((-1.0..=1.0).contains(&v), "v={} out of bounds", v);
        }
        for &xs in &state.x_s {
            assert!((0.0..=1.0).contains(&xs), "x_s={} out of bounds", xs);
        }
        for &xl in &state.x_l {
            assert!(xl >= 1.0, "x_l={} out of bounds", xl);
        }
    }

    #[test]
    fn test_euler_step() {
        let f = test_formula();
        let params = Params::default();
        let mut state = DmmState::new(&f, 42, &params);
        state.init_short_memory(&f);
        let mut derivs = Derivatives::new(f.num_vars, f.num_clauses());
        let mut scratch = ScratchBuffers::empty();

        let dt = integration_step(
            Method::Euler,
            &f,
            &mut state,
            &params,
            &mut derivs,
            &mut scratch,
            -1.0,
        );
        assert!(dt > 0.0);
        assert!(state.t > 0.0);
        assert_bounds(&state);
    }

    #[test]
    fn test_trapezoid_step() {
        let f = test_formula();
        let params = Params::default();
        let mut state = DmmState::new(&f, 42, &params);
        state.init_short_memory(&f);
        let mut derivs = Derivatives::new(f.num_vars, f.num_clauses());
        let mut scratch = ScratchBuffers::new(&f, &state);

        let dt = integration_step(
            Method::Trapezoid,
            &f,
            &mut state,
            &params,
            &mut derivs,
            &mut scratch,
            -1.0,
        );
        assert!(dt > 0.0);
        assert!(state.t > 0.0);
        assert_bounds(&state);
    }

    #[test]
    fn test_rk4_step() {
        let f = test_formula();
        let params = Params::default();
        let mut state = DmmState::new(&f, 42, &params);
        state.init_short_memory(&f);
        let mut derivs = Derivatives::new(f.num_vars, f.num_clauses());
        let mut scratch = ScratchBuffers::new(&f, &state);

        let dt = integration_step(
            Method::Rk4,
            &f,
            &mut state,
            &params,
            &mut derivs,
            &mut scratch,
            -1.0,
        );
        assert!(dt > 0.0);
        assert!(state.t > 0.0);
        assert_bounds(&state);
    }

    #[test]
    fn test_strang_step() {
        let f = test_formula();
        let params = Params::default();
        let mut state = DmmState::new(&f, 42, &params);
        state.init_short_memory(&f);
        let mut derivs = Derivatives::new(f.num_vars, f.num_clauses());
        let mut scratch = ScratchBuffers::new(&f, &state);

        let dt = integration_step(
            Method::Strang,
            &f,
            &mut state,
            &params,
            &mut derivs,
            &mut scratch,
            -1.0,
        );
        assert!(dt > 0.0);
        assert!(state.t > 0.0);
        assert_bounds(&state);
    }

    #[test]
    fn test_bs3_step() {
        let f = test_formula();
        let params = Params::default();
        let mut state = DmmState::new(&f, 42, &params);
        state.init_short_memory(&f);
        let mut derivs = Derivatives::new(f.num_vars, f.num_clauses());
        let mut scratch = ScratchBuffers::new(&f, &state);

        let dt = integration_step(
            Method::Bs3,
            &f,
            &mut state,
            &params,
            &mut derivs,
            &mut scratch,
            -1.0,
        );
        assert!(dt > 0.0);
        assert!(state.t > 0.0);
        assert_bounds(&state);
    }

    #[test]
    fn test_bs3_with_pi() {
        let f = test_formula();
        let params = Params::default();
        let mut state = DmmState::new(&f, 42, &params);
        state.init_short_memory(&f);
        let mut derivs = Derivatives::new(f.num_vars, f.num_clauses());
        let mut scratch = ScratchBuffers::new(&f, &state);
        let mut fsal = FsalState::new(f.num_vars, f.num_clauses());
        let mut pi = PiController::new(params.dt_min, params.dt_max);
        let mut engine = DerivEngine::Loop;

        // Run several steps to exercise FSAL and PI controller
        let mut accepted_count = 0;
        for _ in 0..20 {
            let (dt, _err) = bs3_step_with_pi(
                &f, &mut state, &params, &mut derivs, &mut scratch,
                &mut fsal, &mut pi, &mut engine,
            );
            if dt > 0.0 {
                accepted_count += 1;
            }
        }
        assert!(accepted_count > 0, "BS3+PI should accept some steps");
        assert!(state.t > 0.0);
        assert_bounds(&state);
    }

    #[test]
    fn test_pi_controller() {
        let mut pi = PiController::new(1.0 / 128.0, 1024.0);

        // Small error → step accepted and grows
        let (accepted, _) = pi.propose(0.01);
        assert!(accepted, "Small error should be accepted");
        let dt_after_small = pi.dt;

        // Large error → step rejected and shrinks
        pi.dt = 10.0;
        let (accepted, _) = pi.propose(1.0);
        assert!(!accepted, "Large error should be rejected");
        assert!(pi.dt < 10.0, "Step should shrink after rejection");

        // Zero error → maximum growth
        pi.dt = 1.0;
        let (accepted, _) = pi.propose(0.0);
        assert!(accepted);
        assert!(pi.dt > 1.0, "Zero error should grow step");

        let _ = dt_after_small; // used for assertion above
    }
}
