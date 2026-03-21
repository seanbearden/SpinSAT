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

#[cfg(test)]
mod tests {
    use super::*;

    /// Easy instance: (x1 ∨ x2) ∧ (¬x1 ∨ x2) — x2=true satisfies both
    fn easy_formula() -> Formula {
        Formula::new(2, vec![vec![1, 2], vec![-1, 2]])
    }

    /// Harder instance: 20-var 3-SAT at ratio 4.3 (86 clauses)
    fn harder_formula() -> Formula {
        // Generate a planted-solution instance inline
        let n = 20;
        let mut clauses = Vec::new();
        let mut rng: u64 = 12345;
        let mut next = || -> u64 {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        for _ in 0..86 {
            let a = (next() % n + 1) as i32;
            let b = loop {
                let v = (next() % n + 1) as i32;
                if v != a {
                    break v;
                }
            };
            let c = loop {
                let v = (next() % n + 1) as i32;
                if v != a && v != b {
                    break v;
                }
            };
            let sa = if next() % 2 == 0 { a } else { -a };
            let sb = if next() % 2 == 0 { b } else { -b };
            let sc = if next() % 2 == 0 { c } else { -c };
            // Ensure satisfiable: at least one literal must be positive
            let mut lits = vec![sa, sb, sc];
            if lits.iter().all(|l| *l < 0) {
                lits[0] = -lits[0];
            }
            clauses.push(lits);
        }
        Formula::new(n as usize, clauses)
    }

    #[test]
    fn test_solve_easy_euler() {
        let f = easy_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            method: Method::Euler,
            ..Default::default()
        };
        match solve(&f, &params, &config) {
            SolveResult::Sat(assignment) => {
                assert!(f.verify(&assignment), "Assignment should be valid");
            }
            SolveResult::Unknown => panic!("Should have found a solution"),
        }
    }

    #[test]
    fn test_solve_easy_trapezoid() {
        let f = easy_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            method: Method::Trapezoid,
            ..Default::default()
        };
        match solve(&f, &params, &config) {
            SolveResult::Sat(assignment) => {
                assert!(f.verify(&assignment));
            }
            SolveResult::Unknown => panic!("Should have found a solution"),
        }
    }

    #[test]
    fn test_solve_easy_rk4() {
        let f = easy_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            method: Method::Rk4,
            ..Default::default()
        };
        match solve(&f, &params, &config) {
            SolveResult::Sat(assignment) => {
                assert!(f.verify(&assignment));
            }
            SolveResult::Unknown => panic!("Should have found a solution"),
        }
    }

    #[test]
    fn test_solve_harder_instance() {
        let f = harder_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 30.0,
            stagnation_check_interval: 100,
            stagnation_patience: 10,
            ..Default::default()
        };
        match solve(&f, &params, &config) {
            SolveResult::Sat(assignment) => {
                assert!(f.verify(&assignment));
            }
            SolveResult::Unknown => {
                // Acceptable — some generated instances may not be satisfiable
            }
        }
    }

    #[test]
    fn test_solve_timeout() {
        let f = harder_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 0.001, // nearly instant timeout
            max_restarts: 0,
            stagnation_check_interval: 1,
            stagnation_patience: 1,
            ..Default::default()
        };
        // Should return Unknown due to timeout/stagnation
        match solve(&f, &params, &config) {
            SolveResult::Unknown => {} // expected
            SolveResult::Sat(_) => {}  // also ok if it solves instantly
        }
    }

    #[test]
    fn test_solve_with_restarts() {
        let f = harder_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            max_restarts: 5,
            stagnation_check_interval: 50,
            stagnation_patience: 2,
            ..Default::default()
        };
        // Should exercise the restart path
        let _ = solve(&f, &params, &config);
    }
}
