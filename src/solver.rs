use std::time::Instant;

use crate::dmm::{extract_assignment, is_solved, Derivatives, DmmState, Params};
use crate::formula::Formula;
use crate::integrator::euler_step;

/// Result of a solve attempt.
pub enum SolveResult {
    Sat(Vec<bool>),
    Unknown,
}

/// Main solve loop.
///
/// Integrates the DMM equations until either:
/// - All clause constraints C_m < 0.5 (SAT found)
/// - Wall-clock timeout exceeded (UNKNOWN)
pub fn solve(formula: &Formula, params: &Params, timeout_secs: f64, seed: u64) -> SolveResult {
    let start = Instant::now();
    let mut state = DmmState::new(formula, seed);
    state.init_short_memory(formula);
    let mut derivs = Derivatives::new(formula.num_vars, formula.num_clauses());

    let max_steps: u64 = u64::MAX;
    let mut step: u64 = 0;

    loop {
        // Integration step (adaptive dt)
        euler_step(formula, &mut state, params, &mut derivs, -1.0);
        step += 1;

        // Check solution
        if is_solved(&derivs.c_m) {
            // Verify assignment against original formula
            let assignment = extract_assignment(&state.v);
            if formula.verify(&assignment) {
                return SolveResult::Sat(assignment);
            }
            // Thresholding disagreement — keep integrating
        }

        // Check timeout
        if step.is_multiple_of(1000) {
            let elapsed = start.elapsed().as_secs_f64();
            if elapsed >= timeout_secs {
                return SolveResult::Unknown;
            }
        }

        if step >= max_steps {
            return SolveResult::Unknown;
        }
    }
}
