use std::time::Instant;

use crate::dmm::{count_unsat, extract_assignment, is_solved, Derivatives, DmmState, Params};
use crate::formula::Formula;
use crate::integrator::{integration_step, Method, ScratchBuffers};

/// Result of a solve attempt.
pub enum SolveResult {
    Sat(Vec<bool>),
    Unknown,
}

/// Solver configuration.
pub struct SolverConfig {
    pub timeout_secs: f64,
    pub initial_seed: u64,
    pub max_restarts: u32,
    pub stagnation_check_interval: u64,
    pub stagnation_patience: u32,
    pub method: Method,
}

impl Default for SolverConfig {
    fn default() -> Self {
        SolverConfig {
            timeout_secs: 5000.0,
            initial_seed: 42,
            max_restarts: 1000,
            stagnation_check_interval: 5000,
            stagnation_patience: 20,
            method: Method::Euler,
        }
    }
}

/// Main solve loop with restarts.
pub fn solve(formula: &Formula, params: &Params, config: &SolverConfig) -> SolveResult {
    let start = Instant::now();
    let mut state = DmmState::new(formula, config.initial_seed);
    state.init_short_memory(formula);
    let mut derivs = Derivatives::new(formula.num_vars, formula.num_clauses());

    let mut scratch = match config.method {
        Method::Euler => ScratchBuffers::empty(),
        _ => ScratchBuffers::new(formula, &state),
    };

    let mut restart_count: u32 = 0;
    let mut best_unsat_ever = formula.num_clauses();

    loop {
        let mut step: u64 = 0;
        let mut best_unsat = formula.num_clauses();
        let mut stagnation_counter: u32 = 0;

        loop {
            integration_step(
                config.method,
                formula,
                &mut state,
                params,
                &mut derivs,
                &mut scratch,
                -1.0,
            );
            step += 1;

            if is_solved(&derivs.c_m) {
                let assignment = extract_assignment(&state.v);
                if formula.verify(&assignment) {
                    eprintln!(
                        "c Solved after {} restarts, step {} (t={:.1})",
                        restart_count, step, state.t
                    );
                    return SolveResult::Sat(assignment);
                }
            }

            if step.is_multiple_of(config.stagnation_check_interval) {
                let elapsed = start.elapsed().as_secs_f64();
                if elapsed >= config.timeout_secs {
                    return SolveResult::Unknown;
                }

                let current_unsat = count_unsat(&derivs.c_m);
                if current_unsat < best_unsat {
                    best_unsat = current_unsat;
                    stagnation_counter = 0;
                    if current_unsat < best_unsat_ever {
                        best_unsat_ever = current_unsat;
                    }
                } else {
                    stagnation_counter += 1;
                }

                if stagnation_counter >= config.stagnation_patience {
                    break;
                }
            }
        }

        restart_count += 1;
        if restart_count >= config.max_restarts {
            return SolveResult::Unknown;
        }

        let new_seed = config
            .initial_seed
            .wrapping_add(restart_count as u64 * 7919);

        eprintln!(
            "c Restart {} (best unsat this run: {}, best ever: {}, elapsed: {:.1}s)",
            restart_count,
            best_unsat,
            best_unsat_ever,
            start.elapsed().as_secs_f64()
        );

        state.restart(formula, new_seed);
    }
}
