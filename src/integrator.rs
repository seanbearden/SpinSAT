use crate::dmm::{compute_derivatives, Derivatives, DmmState, Params};
use crate::formula::Formula;

/// Forward Euler integration step with adaptive time step.
///
/// Matches the paper's integration scheme (SeanMethod.m):
/// dt = max(min(dt_max, max_v / max(|dV|)), dt_min)
///
/// Returns the actual dt used.
pub fn euler_step(
    formula: &Formula,
    state: &mut DmmState,
    params: &Params,
    derivs: &mut Derivatives,
    dt: f64,
) -> f64 {
    compute_derivatives(formula, state, params, derivs);

    // Adaptive time step: dt = max(min(dt_max, 1.0 / max(|dV|)), dt_min)
    let actual_dt = if dt < 0.0 {
        let max_dv = derivs.dv.iter().map(|x| x.abs()).fold(0.0f64, f64::max);
        if max_dv > 0.0 {
            (params.dt_max.min(1.0 / max_dv)).max(params.dt_min)
        } else {
            params.dt_max
        }
    } else {
        dt
    };

    // Update long-term memory: x_l += dx_l * dt, clamp to [1, max_xl]
    for m in 0..formula.num_clauses() {
        state.x_l[m] = (state.x_l[m] + derivs.dx_l[m] * actual_dt).clamp(1.0, state.max_xl);
    }

    // Update short-term memory: x_s += dx_s * dt, clamp to [0, 1]
    for m in 0..formula.num_clauses() {
        state.x_s[m] = (state.x_s[m] + derivs.dx_s[m] * actual_dt).clamp(0.0, 1.0);
    }

    // Update voltages: v += dv * dt, clamp to [-1, 1]
    for n in 0..formula.num_vars {
        state.v[n] = (state.v[n] + derivs.dv[n] * actual_dt).clamp(-1.0, 1.0);
    }

    // Track integration time
    state.t += actual_dt;

    // Per-clause α_m adjustment every 10⁴ time units
    if state.t - state.last_alpha_adjust_t >= 1e4 {
        state.adjust_alpha_m();
    }

    actual_dt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::Formula;

    #[test]
    fn test_euler_step_runs() {
        let f = Formula::new(2, vec![vec![1, 2], vec![-1, 2]]);
        let params = Params::default();
        let mut state = DmmState::new(&f, 42);
        state.init_short_memory(&f);
        let mut derivs = Derivatives::new(f.num_vars, f.num_clauses());

        let dt = euler_step(&f, &mut state, &params, &mut derivs, -1.0);
        assert!(dt > 0.0);
        assert!(state.t > 0.0);

        for &v in &state.v {
            assert!((-1.0..=1.0).contains(&v));
        }
        for &xs in &state.x_s {
            assert!((0.0..=1.0).contains(&xs));
        }
        for &xl in &state.x_l {
            assert!(xl >= 1.0);
        }
    }
}
