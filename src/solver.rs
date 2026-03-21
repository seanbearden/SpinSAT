use std::time::Instant;

use crate::dmm::{count_unsat, extract_assignment, is_solved, Derivatives, DmmState, Params};
use crate::formula::Formula;
use crate::integrator::euler_step;

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
    /// Steps before checking for stagnation within a single run
    pub stagnation_check_interval: u64,
    /// If unsat count hasn't improved after this many checks, restart
    pub stagnation_patience: u32,
}

impl Default for SolverConfig {
    fn default() -> Self {
        SolverConfig {
            timeout_secs: 5000.0,
            initial_seed: 42,
            max_restarts: 1000,
            stagnation_check_interval: 5000,
            stagnation_patience: 20,
        }
    }
}

/// Main solve loop with restarts.
///
/// Strategy:
/// 1. Start with initial seed, integrate DMM equations
/// 2. Monitor for stagnation (unsat count not improving)
/// 3. On stagnation, restart with new random ICs (new seed)
/// 4. Keep per-clause α_m across restarts (learned difficulty)
/// 5. Repeat until SAT found or wall-clock timeout
pub fn solve(formula: &Formula, params: &Params, config: &SolverConfig) -> SolveResult {
    let start = Instant::now();
    let mut state = DmmState::new(formula, config.initial_seed);
    state.init_short_memory(formula);
    let mut derivs = Derivatives::new(formula.num_vars, formula.num_clauses());

    let mut restart_count: u32 = 0;
    let mut best_unsat_ever = formula.num_clauses();

    loop {
        let mut step: u64 = 0;
        let mut best_unsat = formula.num_clauses();
        let mut stagnation_counter: u32 = 0;

        // Inner integration loop for this restart attempt
        loop {
            euler_step(formula, &mut state, params, &mut derivs, -1.0);
            step += 1;

            // Check solution
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

            // Periodic checks
            if step.is_multiple_of(config.stagnation_check_interval) {
                // Check wall-clock timeout
                let elapsed = start.elapsed().as_secs_f64();
                if elapsed >= config.timeout_secs {
                    return SolveResult::Unknown;
                }

                // Check for stagnation
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

                // Restart if stagnated
                if stagnation_counter >= config.stagnation_patience {
                    break;
                }
            }
        }

        // Restart with new seed
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

        // Restart: new voltages, reset short-term memory, keep alpha_m
        state.restart(formula, new_seed);
    }
}
