use std::time::Instant;

use crate::cdcl::{CdclResult, CdclSolver};
use crate::dmm::{count_unsat, extract_assignment, is_solved, Derivatives, DmmState, Params};
use crate::formula::Formula;
use crate::integrator::{integration_step, Method, ScratchBuffers};

/// Result of a solve attempt.
pub enum SolveResult {
    Sat(Vec<bool>),
    Unsat,
    Unknown,
}

/// Restart strategy for method selection.
#[derive(Clone, Copy, Debug)]
pub enum Strategy {
    /// Use a single fixed method for all restarts.
    Fixed(Method),
    /// Alternate between Euler and Trapezoid on each restart.
    Alternate,
    /// Probe both methods for a short period, then commit to the faster one.
    Probe,
    /// Track per-method wall-clock effectiveness, bias toward the winner.
    Adaptive,
}

impl Strategy {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "euler" => Some(Strategy::Fixed(Method::Euler)),
            "trapezoid" | "trap" | "heun" => Some(Strategy::Fixed(Method::Trapezoid)),
            "rk4" | "runge-kutta" | "rungekutta" => Some(Strategy::Fixed(Method::Rk4)),
            "alternate" | "alt" => Some(Strategy::Alternate),
            "probe" => Some(Strategy::Probe),
            "adaptive" | "auto" => Some(Strategy::Adaptive),
            _ => None,
        }
    }
}

/// Solver configuration.
pub struct SolverConfig {
    pub timeout_secs: f64,
    pub initial_seed: u64,
    pub max_restarts: u32,
    pub stagnation_check_interval: u64,
    pub stagnation_patience: u32,
    pub strategy: Strategy,
    /// Number of steps for probe strategy's initial test period per method.
    pub probe_steps: u64,
    /// Enable CaDiCaL CDCL fallback for UNSAT detection.
    /// When enabled, hands off to CaDiCaL after DMM exhausts its budget.
    pub cdcl_fallback: bool,
    /// Path for DRAT proof output (only used when cdcl_fallback is enabled).
    pub proof_path: Option<String>,
}

impl Default for SolverConfig {
    fn default() -> Self {
        SolverConfig {
            timeout_secs: 5000.0,
            initial_seed: 42,
            max_restarts: 1000,
            stagnation_check_interval: 5000,
            stagnation_patience: 20,
            strategy: Strategy::Fixed(Method::Euler),
            probe_steps: 5000,
            cdcl_fallback: false,
            proof_path: None,
        }
    }
}

/// Per-method performance tracker for adaptive strategy.
struct MethodStats {
    wall_time: f64,
    best_unsat: usize,
    restarts: u32,
}

impl MethodStats {
    fn new(num_clauses: usize) -> Self {
        MethodStats {
            wall_time: 0.0,
            best_unsat: num_clauses,
            restarts: 0,
        }
    }

    /// Effectiveness: lower unsat per wall-second is better.
    /// Returns unsat reduction rate (higher = better method).
    fn effectiveness(&self, num_clauses: usize) -> f64 {
        if self.wall_time < 1e-6 || self.restarts == 0 {
            return 0.0;
        }
        let reduction = num_clauses as f64 - self.best_unsat as f64;
        reduction / self.wall_time
    }
}

/// Run one restart attempt with the given method.
/// Returns (best_unsat_this_run, wall_time_this_run, solved_assignment_or_none).
fn run_attempt(
    formula: &Formula,
    state: &mut DmmState,
    params: &Params,
    derivs: &mut Derivatives,
    scratch: &mut ScratchBuffers,
    method: Method,
    config: &SolverConfig,
    timeout_deadline: f64,
    start: &Instant,
    max_steps: Option<u64>,
) -> (usize, f64, Option<Vec<bool>>) {
    let attempt_start = start.elapsed().as_secs_f64();
    let mut step: u64 = 0;
    let mut best_unsat = formula.num_clauses();
    let mut stagnation_counter: u32 = 0;
    let step_limit = max_steps.unwrap_or(u64::MAX);

    loop {
        integration_step(method, formula, state, params, derivs, scratch, -1.0);
        step += 1;

        if is_solved(&derivs.c_m) {
            let assignment = extract_assignment(&state.v);
            if formula.verify(&assignment) {
                return (
                    0,
                    start.elapsed().as_secs_f64() - attempt_start,
                    Some(assignment),
                );
            }
        }

        if step >= step_limit {
            break;
        }

        if step.is_multiple_of(config.stagnation_check_interval) {
            let elapsed = start.elapsed().as_secs_f64();
            if elapsed >= timeout_deadline {
                break;
            }

            let current_unsat = count_unsat(&derivs.c_m);
            if current_unsat < best_unsat {
                best_unsat = current_unsat;
                stagnation_counter = 0;
            } else {
                stagnation_counter += 1;
            }

            if stagnation_counter >= config.stagnation_patience {
                break;
            }
        }
    }

    let wall_time = start.elapsed().as_secs_f64() - attempt_start;
    (best_unsat, wall_time, None)
}

/// Select which method to use for a given restart, based on strategy.
fn select_method(
    strategy: Strategy,
    restart_count: u32,
    euler_stats: &MethodStats,
    trap_stats: &MethodStats,
    num_clauses: usize,
    probe_complete: bool,
    probe_winner: Option<Method>,
) -> Method {
    match strategy {
        Strategy::Fixed(m) => m,
        Strategy::Alternate => {
            if restart_count % 2 == 0 {
                Method::Euler
            } else {
                Method::Trapezoid
            }
        }
        Strategy::Probe => {
            if probe_complete {
                probe_winner.unwrap_or(Method::Euler)
            } else {
                // During probe phase: restart 0 = Euler, restart 1 = Trapezoid
                if restart_count == 0 {
                    Method::Euler
                } else {
                    Method::Trapezoid
                }
            }
        }
        Strategy::Adaptive => {
            // Need at least one attempt with each before comparing
            if euler_stats.restarts == 0 {
                Method::Euler
            } else if trap_stats.restarts == 0 {
                Method::Trapezoid
            } else {
                let euler_eff = euler_stats.effectiveness(num_clauses);
                let trap_eff = trap_stats.effectiveness(num_clauses);
                if trap_eff > euler_eff {
                    Method::Trapezoid
                } else {
                    Method::Euler
                }
            }
        }
    }
}

/// Main solve loop with restarts and strategy-based method selection.
pub fn solve(formula: &Formula, params: &Params, config: &SolverConfig) -> SolveResult {
    let start = Instant::now();
    let mut state = DmmState::new(formula, config.initial_seed);
    state.init_short_memory(formula);
    let mut derivs = Derivatives::new(formula.num_vars, formula.num_clauses());

    // Pre-allocate scratch for both Euler and Trapezoid
    let needs_scratch = !matches!(config.strategy, Strategy::Fixed(Method::Euler));
    let mut scratch = if needs_scratch {
        ScratchBuffers::new(formula, &state)
    } else {
        ScratchBuffers::empty()
    };

    let mut restart_count: u32 = 0;
    let mut best_unsat_ever = formula.num_clauses();
    let mut best_voltages: Vec<f64> = state.v.clone();
    let mut euler_stats = MethodStats::new(formula.num_clauses());
    let mut trap_stats = MethodStats::new(formula.num_clauses());
    let mut probe_complete = false;
    let mut probe_winner: Option<Method> = None;

    // When CDCL fallback is enabled, reserve time for CaDiCaL
    let dmm_timeout = if config.cdcl_fallback {
        // Give DMM 50% of the time budget, CaDiCaL gets the rest
        config.timeout_secs * 0.5
    } else {
        config.timeout_secs
    };

    loop {
        // Select method for this restart
        let method = select_method(
            config.strategy,
            restart_count,
            &euler_stats,
            &trap_stats,
            formula.num_clauses(),
            probe_complete,
            probe_winner,
        );

        // Ensure scratch buffers are allocated if using non-Euler method
        if !matches!(method, Method::Euler) && scratch.tmp_state.is_none() {
            scratch = ScratchBuffers::new(formula, &state);
        }

        // For probe strategy, limit steps during probe phase
        let max_steps = if matches!(config.strategy, Strategy::Probe) && !probe_complete {
            Some(config.probe_steps)
        } else {
            None
        };

        let (best_unsat, wall_time, solution) = run_attempt(
            formula,
            &mut state,
            params,
            &mut derivs,
            &mut scratch,
            method,
            config,
            dmm_timeout,
            &start,
            max_steps,
        );

        // Check if solved
        if let Some(assignment) = solution {
            eprintln!(
                "c Solved after {} restarts using {:?} (elapsed: {:.1}s)",
                restart_count,
                method,
                start.elapsed().as_secs_f64()
            );
            return SolveResult::Sat(assignment);
        }

        // Track best voltages across all restarts
        if best_unsat < best_unsat_ever {
            best_unsat_ever = best_unsat;
            best_voltages = state.v.clone();
        }

        // Update stats
        match method {
            Method::Euler => {
                euler_stats.wall_time += wall_time;
                euler_stats.restarts += 1;
                if best_unsat < euler_stats.best_unsat {
                    euler_stats.best_unsat = best_unsat;
                }
            }
            Method::Trapezoid => {
                trap_stats.wall_time += wall_time;
                trap_stats.restarts += 1;
                if best_unsat < trap_stats.best_unsat {
                    trap_stats.best_unsat = best_unsat;
                }
            }
            _ => {}
        }

        // Probe strategy: after both methods have been probed, pick the winner
        if matches!(config.strategy, Strategy::Probe) && !probe_complete && restart_count == 1 {
            probe_complete = true;
            probe_winner = Some(if euler_stats.best_unsat <= trap_stats.best_unsat {
                Method::Euler
            } else {
                Method::Trapezoid
            });
            eprintln!(
                "c Probe complete: Euler best_unsat={}, Trap best_unsat={} → {:?}",
                euler_stats.best_unsat,
                trap_stats.best_unsat,
                probe_winner.unwrap()
            );
        }

        // Check DMM timeout
        if start.elapsed().as_secs_f64() >= dmm_timeout {
            break;
        }

        restart_count += 1;
        if restart_count >= config.max_restarts {
            break;
        }

        let new_seed = config
            .initial_seed
            .wrapping_add(restart_count as u64 * 7919);

        eprintln!(
            "c Restart {} {:?} (best unsat: {}, best ever: {}, elapsed: {:.1}s)",
            restart_count,
            method,
            best_unsat,
            best_unsat_ever,
            start.elapsed().as_secs_f64()
        );

        state.restart(formula, new_seed);
    }

    // DMM exhausted its budget without finding SAT
    if config.cdcl_fallback {
        cdcl_fallback(formula, &best_voltages, best_unsat_ever, config, &start)
    } else {
        SolveResult::Unknown
    }
}

/// Hand off to CaDiCaL CDCL solver after DMM exhausts its budget.
///
/// Seeds CaDiCaL with the DMM's best voltage assignment as phase hints,
/// following the Deep Cooperation approach (Cai et al., IJCAI 2022).
fn cdcl_fallback(
    formula: &Formula,
    best_voltages: &[f64],
    best_unsat: usize,
    config: &SolverConfig,
    start: &Instant,
) -> SolveResult {
    let remaining = config.timeout_secs - start.elapsed().as_secs_f64();
    if remaining <= 0.0 {
        return SolveResult::Unknown;
    }

    eprintln!(
        "c CDCL fallback: DMM best_unsat={}, handing off with {:.1}s remaining",
        best_unsat, remaining
    );

    let mut cdcl = CdclSolver::new(formula);

    // Set phase hints from DMM's best voltage assignment (Deep Cooperation: LS Rephasing)
    cdcl.set_phase_from_voltages(best_voltages);

    // Enable DRAT proof output if requested
    if let Some(ref path) = config.proof_path {
        cdcl.enable_proof(path);
    }

    // Solve with CaDiCaL
    let result = cdcl.solve();

    // Close proof trace if enabled
    if config.proof_path.is_some() {
        cdcl.close_proof();
    }

    match result {
        CdclResult::Sat(assignment) => {
            if formula.verify(&assignment) {
                eprintln!(
                    "c CDCL found SAT (elapsed: {:.1}s)",
                    start.elapsed().as_secs_f64()
                );
                SolveResult::Sat(assignment)
            } else {
                eprintln!("c CDCL returned invalid SAT assignment, reporting UNKNOWN");
                SolveResult::Unknown
            }
        }
        CdclResult::Unsat => {
            eprintln!(
                "c CDCL proved UNSAT (elapsed: {:.1}s)",
                start.elapsed().as_secs_f64()
            );
            SolveResult::Unsat
        }
        CdclResult::Unknown => SolveResult::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn easy_formula() -> Formula {
        Formula::new(2, vec![vec![1, 2], vec![-1, 2]])
    }

    fn harder_formula() -> Formula {
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
            strategy: Strategy::Fixed(Method::Euler),
            ..Default::default()
        };
        match solve(&f, &params, &config) {
            SolveResult::Sat(a) => assert!(f.verify(&a)),
            SolveResult::Unsat => panic!("Should have found a solution"),
            SolveResult::Unknown => panic!("Should have found a solution"),
        }
    }

    #[test]
    fn test_solve_easy_trapezoid() {
        let f = easy_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            strategy: Strategy::Fixed(Method::Trapezoid),
            ..Default::default()
        };
        match solve(&f, &params, &config) {
            SolveResult::Sat(a) => assert!(f.verify(&a)),
            SolveResult::Unsat => panic!("Should have found a solution"),
            SolveResult::Unknown => panic!("Should have found a solution"),
        }
    }

    #[test]
    fn test_solve_easy_rk4() {
        let f = easy_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            strategy: Strategy::Fixed(Method::Rk4),
            ..Default::default()
        };
        match solve(&f, &params, &config) {
            SolveResult::Sat(a) => assert!(f.verify(&a)),
            SolveResult::Unsat => panic!("Should have found a solution"),
            SolveResult::Unknown => panic!("Should have found a solution"),
        }
    }

    #[test]
    fn test_solve_alternate() {
        let f = harder_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            strategy: Strategy::Alternate,
            stagnation_check_interval: 50,
            stagnation_patience: 2,
            max_restarts: 10,
            ..Default::default()
        };
        let _ = solve(&f, &params, &config);
    }

    #[test]
    fn test_solve_probe() {
        let f = harder_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            strategy: Strategy::Probe,
            probe_steps: 100,
            stagnation_check_interval: 50,
            stagnation_patience: 2,
            max_restarts: 10,
            ..Default::default()
        };
        let _ = solve(&f, &params, &config);
    }

    #[test]
    fn test_solve_adaptive() {
        let f = harder_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            strategy: Strategy::Adaptive,
            stagnation_check_interval: 50,
            stagnation_patience: 2,
            max_restarts: 10,
            ..Default::default()
        };
        let _ = solve(&f, &params, &config);
    }

    #[test]
    fn test_solve_timeout() {
        let f = harder_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 0.001,
            max_restarts: 0,
            stagnation_check_interval: 1,
            stagnation_patience: 1,
            ..Default::default()
        };
        match solve(&f, &params, &config) {
            SolveResult::Unknown | SolveResult::Unsat | SolveResult::Sat(_) => {}
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
        let _ = solve(&f, &params, &config);
    }

    #[test]
    fn test_strategy_from_str() {
        assert!(matches!(
            Strategy::from_str("euler"),
            Some(Strategy::Fixed(Method::Euler))
        ));
        assert!(matches!(
            Strategy::from_str("trapezoid"),
            Some(Strategy::Fixed(Method::Trapezoid))
        ));
        assert!(matches!(
            Strategy::from_str("alternate"),
            Some(Strategy::Alternate)
        ));
        assert!(matches!(Strategy::from_str("probe"), Some(Strategy::Probe)));
        assert!(matches!(
            Strategy::from_str("auto"),
            Some(Strategy::Adaptive)
        ));
        assert!(Strategy::from_str("invalid").is_none());
    }

    #[test]
    fn test_cdcl_fallback_sat() {
        // Easy SAT formula — DMM should solve it, but test the fallback path
        let f = easy_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            cdcl_fallback: true,
            strategy: Strategy::Fixed(Method::Euler),
            ..Default::default()
        };
        match solve(&f, &params, &config) {
            SolveResult::Sat(a) => assert!(f.verify(&a)),
            SolveResult::Unsat => panic!("Easy formula should be SAT"),
            SolveResult::Unknown => panic!("Should have found a solution"),
        }
    }

    #[test]
    fn test_cdcl_fallback_unsat() {
        // Trivially UNSAT: (x1) AND (NOT x1)
        let f = Formula::new(1, vec![vec![1], vec![-1]]);
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            cdcl_fallback: true,
            max_restarts: 2,
            stagnation_check_interval: 10,
            stagnation_patience: 1,
            strategy: Strategy::Fixed(Method::Euler),
            ..Default::default()
        };
        match solve(&f, &params, &config) {
            SolveResult::Unsat => {} // expected
            SolveResult::Sat(_) => panic!("UNSAT formula should not be SAT"),
            SolveResult::Unknown => panic!("CDCL fallback should prove UNSAT"),
        }
    }

    #[test]
    fn test_cdcl_fallback_harder_unsat() {
        // UNSAT: (x1 OR x2) AND (NOT x1 OR x2) AND (x1 OR NOT x2) AND (NOT x1 OR NOT x2)
        let f = Formula::new(2, vec![vec![1, 2], vec![-1, 2], vec![1, -2], vec![-1, -2]]);
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            cdcl_fallback: true,
            max_restarts: 5,
            stagnation_check_interval: 50,
            stagnation_patience: 2,
            strategy: Strategy::Fixed(Method::Euler),
            ..Default::default()
        };
        match solve(&f, &params, &config) {
            SolveResult::Unsat => {} // expected
            SolveResult::Sat(_) => panic!("UNSAT formula should not be SAT"),
            SolveResult::Unknown => panic!("CDCL fallback should prove UNSAT"),
        }
    }
}
