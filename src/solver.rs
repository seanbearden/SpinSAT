use std::time::Instant;

use crate::cdcl::{CdclResult, CdclSolver};
use crate::dmm::{count_unsat, extract_assignment, is_solved, Derivatives, DmmState, Params};
use crate::formula::Formula;
use crate::integrator::{integration_step, Method, ScratchBuffers};
use crate::unsat_signal::{SignalConfig, SignalKind, UnsatSignalDetector};

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
    /// Enable UNSAT signal detection with CaDiCaL handoff.
    /// When enabled, monitors DMM dynamics and hands off to CaDiCaL
    /// when UNSAT indicator signals fire (before DMM budget exhaustion).
    pub enable_unsat_detection: bool,
    /// Configuration for UNSAT signal thresholds.
    pub signal_config: SignalConfig,
    /// CaDiCaL conflict budget per signal-triggered handoff attempt.
    pub cdcl_conflict_budget: i32,
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
            enable_unsat_detection: false,
            signal_config: SignalConfig::default(),
            cdcl_conflict_budget: 100_000,
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

/// Result of a single restart attempt.
struct AttemptResult {
    best_unsat: usize,
    wall_time: f64,
    solution: Option<Vec<bool>>,
    signal_fired: Option<SignalKind>,
}

/// Run one restart attempt with the given method.
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
    mut signal_detector: Option<&mut UnsatSignalDetector>,
) -> AttemptResult {
    let attempt_start = start.elapsed().as_secs_f64();
    let mut step: u64 = 0;
    let mut best_unsat = formula.num_clauses();
    let mut stagnation_counter: u32 = 0;
    let step_limit = max_steps.unwrap_or(u64::MAX);
    let mut signal_fired: Option<SignalKind> = None;

    loop {
        integration_step(method, formula, state, params, derivs, scratch, -1.0);
        step += 1;

        if is_solved(&derivs.c_m) {
            let assignment = extract_assignment(&state.v);
            if formula.verify(&assignment) {
                return AttemptResult {
                    best_unsat: 0,
                    wall_time: start.elapsed().as_secs_f64() - attempt_start,
                    solution: Some(assignment),
                    signal_fired: None,
                };
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

            // Update UNSAT signal detector
            if let Some(ref mut detector) = signal_detector {
                if let Some(signal) = detector.update(state, &derivs.c_m) {
                    signal_fired = Some(signal);
                    break;
                }
            }
        }
    }

    let wall_time = start.elapsed().as_secs_f64() - attempt_start;
    AttemptResult {
        best_unsat,
        wall_time,
        solution: None,
        signal_fired,
    }
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
                if restart_count == 0 {
                    Method::Euler
                } else {
                    Method::Trapezoid
                }
            }
        }
        Strategy::Adaptive => {
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

/// Feedback from CaDiCaL to DMM after a bounded handoff attempt.
pub struct CdclFeedback {
    /// Unit clauses: variables CaDiCaL proved must have a specific value.
    /// Each element is a 1-based signed literal.
    pub fixed_literals: Vec<i32>,
    /// CaDiCaL's phase assignments as voltages for DMM restart.
    /// None if CaDiCaL didn't reach a SAT state (Unknown or UNSAT).
    pub voltages: Option<Vec<f64>>,
}

/// Attempt CaDiCaL handoff with phase hints from DMM's best assignment.
/// Returns (result, feedback):
/// - result: Some(SolveResult) if CaDiCaL resolves it, None if budget exhausted
/// - feedback: CaDiCaL's learned info for DMM (always returned, even on Unknown)
fn try_cdcl_handoff(
    formula: &Formula,
    best_assignment: &[bool],
    conflict_budget: i32,
    proof_path: Option<&str>,
) -> (Option<SolveResult>, CdclFeedback) {
    let mut cdcl = CdclSolver::with_proof(formula, proof_path);
    cdcl.set_phase_from_assignment(best_assignment);
    cdcl.set_conflict_limit(conflict_budget);

    let result = match cdcl.solve() {
        CdclResult::Sat(assignment) => {
            if formula.verify(&assignment) {
                Some(SolveResult::Sat(assignment))
            } else {
                None
            }
        }
        CdclResult::Unsat => Some(SolveResult::Unsat),
        CdclResult::Unknown => None,
    };

    if proof_path.is_some() {
        cdcl.close_proof();
    }

    let feedback = CdclFeedback {
        fixed_literals: cdcl.get_fixed_literals(),
        voltages: cdcl.get_phases_as_voltages(),
    };

    (result, feedback)
}

/// Main solve loop with restarts and strategy-based method selection.
pub fn solve(formula: &mut Formula, params: &Params, config: &SolverConfig) -> SolveResult {
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

    // UNSAT signal detector
    let mut signal_detector = if config.enable_unsat_detection {
        Some(UnsatSignalDetector::new(
            formula.num_vars,
            formula.num_clauses(),
            config.signal_config.clone(),
        ))
    } else {
        None
    };
    let mut cdcl_handoff_count: u32 = 0;

    let mut restart_count: u32 = 0;
    let mut best_unsat_ever = formula.num_clauses();
    let mut best_voltages: Vec<f64> = state.v.clone();
    let mut euler_stats = MethodStats::new(formula.num_clauses());
    let mut trap_stats = MethodStats::new(formula.num_clauses());
    let mut probe_complete = false;
    let mut probe_winner: Option<Method> = None;

    // When CDCL fallback is enabled, reserve time for CaDiCaL
    let dmm_timeout = if config.cdcl_fallback {
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

        let attempt = run_attempt(
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
            signal_detector.as_mut(),
        );

        // Check if solved
        if let Some(assignment) = attempt.solution {
            eprintln!(
                "c Solved after {} restarts using {:?} (elapsed: {:.1}s)",
                restart_count,
                method,
                start.elapsed().as_secs_f64()
            );
            return SolveResult::Sat(assignment);
        }

        // Handle UNSAT signal: hand off to CaDiCaL with conflict budget
        if let Some(signal) = attempt.signal_fired {
            cdcl_handoff_count += 1;
            let best_assign = signal_detector.as_ref().unwrap().best_assignment().to_vec();
            let summary = signal_detector.as_ref().unwrap().signal_summary();
            eprintln!(
                "c UNSAT signal {:?} fired (handoff #{}, elapsed: {:.1}s, {})",
                signal,
                cdcl_handoff_count,
                start.elapsed().as_secs_f64(),
                summary,
            );

            let (result, feedback) = try_cdcl_handoff(
                formula,
                &best_assign,
                config.cdcl_conflict_budget,
                config.proof_path.as_deref(),
            );

            if let Some(result) = result {
                match &result {
                    SolveResult::Sat(_) => {
                        eprintln!(
                            "c CaDiCaL found SAT (handoff #{}, elapsed: {:.1}s)",
                            cdcl_handoff_count,
                            start.elapsed().as_secs_f64()
                        );
                    }
                    SolveResult::Unsat => {
                        eprintln!(
                            "c CaDiCaL proved UNSAT (handoff #{}, elapsed: {:.1}s)",
                            cdcl_handoff_count,
                            start.elapsed().as_secs_f64()
                        );
                    }
                    SolveResult::Unknown => unreachable!(),
                }
                return result;
            }

            // --- Bidirectional feedback: CaDiCaL → DMM ---

            // Add fixed literals as unit clauses to the formula
            let num_fixed = feedback.fixed_literals.len();
            for &lit in &feedback.fixed_literals {
                formula.add_clause(&[lit]);
            }

            // Smart restart: use CaDiCaL's phases as initial voltages if available,
            // otherwise fall back to the DMM's best voltages
            if let Some(ref cdcl_voltages) = feedback.voltages {
                state.restart_with_feedback(formula, cdcl_voltages);
            } else {
                state.restart_with_feedback(formula, &best_voltages);
            }

            // Reallocate derivatives and scratch for potentially larger formula
            derivs = Derivatives::new(formula.num_vars, formula.num_clauses());
            let needs_scratch = !matches!(config.strategy, Strategy::Fixed(Method::Euler));
            scratch = if needs_scratch {
                ScratchBuffers::new(formula, &state)
            } else {
                ScratchBuffers::empty()
            };

            eprintln!(
                "c CaDiCaL exhausted budget ({} conflicts), resuming DMM with {} fixed lits, {} total clauses",
                config.cdcl_conflict_budget,
                num_fixed,
                formula.num_clauses(),
            );

            // Reset signal detector for next DMM phase
            if let Some(ref mut detector) = signal_detector {
                detector.reset_for_restart();
            }

            // Skip the normal restart logic below — we already did a smart restart
            restart_count += 1;
            continue;
        }

        // Track best voltages across all restarts
        if attempt.best_unsat < best_unsat_ever {
            best_unsat_ever = attempt.best_unsat;
            best_voltages = state.v.clone();
        }

        // Update stats
        match method {
            Method::Euler => {
                euler_stats.wall_time += attempt.wall_time;
                euler_stats.restarts += 1;
                if attempt.best_unsat < euler_stats.best_unsat {
                    euler_stats.best_unsat = attempt.best_unsat;
                }
            }
            Method::Trapezoid => {
                trap_stats.wall_time += attempt.wall_time;
                trap_stats.restarts += 1;
                if attempt.best_unsat < trap_stats.best_unsat {
                    trap_stats.best_unsat = attempt.best_unsat;
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
            attempt.best_unsat,
            best_unsat_ever,
            start.elapsed().as_secs_f64()
        );

        state.restart(formula, new_seed);

        // Reset signal detector counters for new restart
        if let Some(ref mut detector) = signal_detector {
            detector.reset_for_restart();
        }
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

    let mut cdcl = CdclSolver::with_proof(formula, config.proof_path.as_deref());

    // Set phase hints from DMM's best voltage assignment (Deep Cooperation: LS Rephasing)
    cdcl.set_phase_from_voltages(best_voltages);

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
        let mut f = easy_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            strategy: Strategy::Fixed(Method::Euler),
            ..Default::default()
        };
        match solve(&mut f, &params, &config) {
            SolveResult::Sat(a) => assert!(f.verify(&a)),
            SolveResult::Unsat | SolveResult::Unknown => panic!("Should have found a solution"),
        }
    }

    #[test]
    fn test_solve_easy_trapezoid() {
        let mut f = easy_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            strategy: Strategy::Fixed(Method::Trapezoid),
            ..Default::default()
        };
        match solve(&mut f, &params, &config) {
            SolveResult::Sat(a) => assert!(f.verify(&a)),
            SolveResult::Unsat | SolveResult::Unknown => panic!("Should have found a solution"),
        }
    }

    #[test]
    fn test_solve_easy_rk4() {
        let mut f = easy_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            strategy: Strategy::Fixed(Method::Rk4),
            ..Default::default()
        };
        match solve(&mut f, &params, &config) {
            SolveResult::Sat(a) => assert!(f.verify(&a)),
            SolveResult::Unsat | SolveResult::Unknown => panic!("Should have found a solution"),
        }
    }

    #[test]
    fn test_solve_alternate() {
        let mut f = harder_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            strategy: Strategy::Alternate,
            stagnation_check_interval: 50,
            stagnation_patience: 2,
            max_restarts: 10,
            ..Default::default()
        };
        let _ = solve(&mut f, &params, &config);
    }

    #[test]
    fn test_solve_probe() {
        let mut f = harder_formula();
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
        let _ = solve(&mut f, &params, &config);
    }

    #[test]
    fn test_solve_adaptive() {
        let mut f = harder_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            strategy: Strategy::Adaptive,
            stagnation_check_interval: 50,
            stagnation_patience: 2,
            max_restarts: 10,
            ..Default::default()
        };
        let _ = solve(&mut f, &params, &config);
    }

    #[test]
    fn test_solve_timeout() {
        let mut f = harder_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 0.001,
            max_restarts: 0,
            stagnation_check_interval: 1,
            stagnation_patience: 1,
            ..Default::default()
        };
        match solve(&mut f, &params, &config) {
            SolveResult::Unknown | SolveResult::Unsat | SolveResult::Sat(_) => {}
        }
    }

    #[test]
    fn test_solve_with_restarts() {
        let mut f = harder_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            max_restarts: 5,
            stagnation_check_interval: 50,
            stagnation_patience: 2,
            ..Default::default()
        };
        let _ = solve(&mut f, &params, &config);
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
        let mut f = easy_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            cdcl_fallback: true,
            strategy: Strategy::Fixed(Method::Euler),
            ..Default::default()
        };
        match solve(&mut f, &params, &config) {
            SolveResult::Sat(a) => assert!(f.verify(&a)),
            SolveResult::Unsat => panic!("Easy formula should be SAT"),
            SolveResult::Unknown => panic!("Should have found a solution"),
        }
    }

    #[test]
    fn test_cdcl_fallback_unsat() {
        let mut f = Formula::new(1, vec![vec![1], vec![-1]]);
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
        match solve(&mut f, &params, &config) {
            SolveResult::Unsat => {}
            SolveResult::Sat(_) => panic!("UNSAT formula should not be SAT"),
            SolveResult::Unknown => panic!("CDCL fallback should prove UNSAT"),
        }
    }

    #[test]
    fn test_cdcl_fallback_harder_unsat() {
        let mut f = Formula::new(2, vec![vec![1, 2], vec![-1, 2], vec![1, -2], vec![-1, -2]]);
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
        match solve(&mut f, &params, &config) {
            SolveResult::Unsat => {}
            SolveResult::Sat(_) => panic!("UNSAT formula should not be SAT"),
            SolveResult::Unknown => panic!("CDCL fallback should prove UNSAT"),
        }
    }

    #[test]
    fn test_solve_unsat_with_signal_detection() {
        // Trivially unsatisfiable: (x1) AND (NOT x1)
        let mut f = Formula::new(1, vec![vec![1], vec![-1]]);
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            max_restarts: 5,
            stagnation_check_interval: 10,
            stagnation_patience: 3,
            enable_unsat_detection: true,
            signal_config: SignalConfig {
                warmup_checks: 1,
                stagnation_patience: 2,
                xl_reset_fraction: 0.5,
                alpha_m_mean_threshold: 50.0,
                alpha_divergence_patience: 5,
                assignment_stability_patience: 3,
            },
            cdcl_conflict_budget: 10_000,
            ..Default::default()
        };
        match solve(&mut f, &params, &config) {
            SolveResult::Unsat => {} // expected — CaDiCaL proves UNSAT
            SolveResult::Sat(_) => panic!("Should not find SAT for UNSAT formula"),
            SolveResult::Unknown => {} // acceptable if signal doesn't fire in time
        }
    }

    #[test]
    fn test_solve_sat_with_signal_detection_enabled() {
        // SAT formula with detection enabled — should still find SAT
        let mut f = easy_formula();
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            enable_unsat_detection: true,
            signal_config: SignalConfig::default(),
            ..Default::default()
        };
        match solve(&mut f, &params, &config) {
            SolveResult::Sat(a) => assert!(f.verify(&a)),
            SolveResult::Unsat => panic!("Should not prove UNSAT for SAT formula"),
            SolveResult::Unknown => panic!("Should have found a solution"),
        }
    }

    #[test]
    fn test_drat_proof_output() {
        // UNSAT formula with DRAT proof output
        let mut f = Formula::new(1, vec![vec![1], vec![-1]]);
        let params = Params::default();
        let proof_file = std::env::temp_dir().join("spinsat_test_proof.drat");
        let proof_path = proof_file.to_str().unwrap().to_string();

        let config = SolverConfig {
            timeout_secs: 10.0,
            cdcl_fallback: true,
            proof_path: Some(proof_path.clone()),
            max_restarts: 2,
            stagnation_check_interval: 10,
            stagnation_patience: 1,
            strategy: Strategy::Fixed(Method::Euler),
            ..Default::default()
        };
        match solve(&mut f, &params, &config) {
            SolveResult::Unsat => {
                // Verify proof file was created and is non-empty
                let metadata = std::fs::metadata(&proof_path);
                assert!(metadata.is_ok(), "Proof file should exist");
                assert!(metadata.unwrap().len() > 0, "Proof file should be non-empty");
            }
            other => panic!("Expected UNSAT, got {:?}", matches!(other, SolveResult::Sat(_))),
        }
        // Clean up
        let _ = std::fs::remove_file(&proof_path);
    }
}
