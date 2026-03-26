use crate::dmm::{compute_derivatives, Derivatives, DmmState, Params};
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
#[derive(Clone, Copy, Debug)]
pub enum Method {
    Euler,
    Trapezoid,
    Rk4,
}

impl Method {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "euler" => Some(Method::Euler),
            "trapezoid" | "trap" | "heun" => Some(Method::Trapezoid),
            "rk4" | "runge-kutta" | "rungekutta" => Some(Method::Rk4),
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
fn set_tmp_state(
    tmp: &mut DmmState,
    base: &DmmState,
    dv: &[f64],
    dx_s: &[f64],
    dx_l: &[f64],
    dt: f64,
) {
    for i in 0..base.v.len() {
        tmp.v[i] = (base.v[i] + dt * dv[i]).clamp(-1.0, 1.0);
    }
    for i in 0..base.x_s.len() {
        tmp.x_s[i] = (base.x_s[i] + dt * dx_s[i]).clamp(0.0, 1.0);
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
        state.x_s[i] = (state.x_s[i] + derivs.dx_s[i] * actual_dt).clamp(0.0, 1.0);
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
        tmp,
        state,
        &derivs.dv,
        &derivs.dx_s,
        &derivs.dx_l,
        actual_dt,
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
        state.x_s[i] = (state.x_s[i] + half_dt * (derivs.dx_s[i] + d2.dx_s[i])).clamp(0.0, 1.0);
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
    set_tmp_state(tmp, state, &derivs.dv, &derivs.dx_s, &derivs.dx_l, half_dt);
    let d2 = scratch.d2.as_mut().unwrap();
    compute_derivatives(formula, tmp, params, d2);

    // Stage 3: k3 at y + dt/2 * k2
    set_tmp_state(tmp, state, &d2.dv, &d2.dx_s, &d2.dx_l, half_dt);
    let d3 = scratch.d3.as_mut().unwrap();
    compute_derivatives(formula, tmp, params, d3);

    // Stage 4: k4 at y + dt * k3
    set_tmp_state(tmp, state, &d3.dv, &d3.dx_s, &d3.dx_l, actual_dt);
    let d4 = scratch.d4.as_mut().unwrap();
    compute_derivatives(formula, tmp, params, d4);

    // Update: y += dt/6 * (k1 + 2*k2 + 2*k3 + k4)
    let dt_sixth = actual_dt / 6.0;
    for i in 0..m {
        let dx = derivs.dx_l[i] + 2.0 * d2.dx_l[i] + 2.0 * d3.dx_l[i] + d4.dx_l[i];
        state.x_l[i] = (state.x_l[i] + dt_sixth * dx).clamp(1.0, state.max_xl);
    }
    for i in 0..m {
        let dx = derivs.dx_s[i] + 2.0 * d2.dx_s[i] + 2.0 * d3.dx_s[i] + d4.dx_s[i];
        state.x_s[i] = (state.x_s[i] + dt_sixth * dx).clamp(0.0, 1.0);
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
        state.x_s[i] = (state.x_s[i] + derivs.dx_s[i] * actual_dt).clamp(0.0, 1.0);
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
    set_tmp_state(tmp, state, &derivs.dv, &derivs.dx_s, &derivs.dx_l, actual_dt);
    let d2 = scratch.d2.as_mut().unwrap();
    engine.compute(formula, tmp, params, d2);

    let half_dt = actual_dt * 0.5;
    for i in 0..m {
        state.x_l[i] =
            (state.x_l[i] + half_dt * (derivs.dx_l[i] + d2.dx_l[i])).clamp(1.0, state.max_xl);
    }
    for i in 0..m {
        state.x_s[i] = (state.x_s[i] + half_dt * (derivs.dx_s[i] + d2.dx_s[i])).clamp(0.0, 1.0);
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

    set_tmp_state(tmp, state, &derivs.dv, &derivs.dx_s, &derivs.dx_l, half_dt);
    let d2 = scratch.d2.as_mut().unwrap();
    engine.compute(formula, tmp, params, d2);

    set_tmp_state(tmp, state, &d2.dv, &d2.dx_s, &d2.dx_l, half_dt);
    let d3 = scratch.d3.as_mut().unwrap();
    engine.compute(formula, tmp, params, d3);

    set_tmp_state(tmp, state, &d3.dv, &d3.dx_s, &d3.dx_l, actual_dt);
    let d4 = scratch.d4.as_mut().unwrap();
    engine.compute(formula, tmp, params, d4);

    let dt_sixth = actual_dt / 6.0;
    for i in 0..m {
        let dx = derivs.dx_l[i] + 2.0 * d2.dx_l[i] + 2.0 * d3.dx_l[i] + d4.dx_l[i];
        state.x_l[i] = (state.x_l[i] + dt_sixth * dx).clamp(1.0, state.max_xl);
    }
    for i in 0..m {
        let dx = derivs.dx_s[i] + 2.0 * d2.dx_s[i] + 2.0 * d3.dx_s[i] + d4.dx_s[i];
        state.x_s[i] = (state.x_s[i] + dt_sixth * dx).clamp(0.0, 1.0);
    }
    for i in 0..n {
        let dx = derivs.dv[i] + 2.0 * d2.dv[i] + 2.0 * d3.dv[i] + d4.dv[i];
        state.v[i] = (state.v[i] + dt_sixth * dx).clamp(-1.0, 1.0);
    }
    derivs.c_m.copy_from_slice(&d4.c_m);
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
}
