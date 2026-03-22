//! UNSAT signal detection for hybrid DMM-CDCL cooperation.
//!
//! Monitors DMM dynamics for signals that suggest the instance may be UNSAT.
//! When a signal fires, the solver should switch to CaDiCaL for definitive
//! SAT/UNSAT determination.
//!
//! Four candidate signals (from HYBRID_DMM_CDCL_REQUIREMENTS.md):
//! 1. **x_l reset saturation**: Too many clauses have had x_{l,m} hit max and reset
//! 2. **C(v) stagnation**: Best unsatisfied clause count hasn't improved
//! 3. **α_m divergence**: Per-clause α_m values growing unboundedly
//! 4. **Best assignment stability**: Best-seen assignment unchanged for too long

use crate::dmm::{count_unsat, extract_assignment, DmmState};

/// Which signal fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    XlResetSaturation,
    CvStagnation,
    AlphaMDivergence,
    BestAssignmentStability,
}

/// Configuration for UNSAT signal detection thresholds.
#[derive(Debug, Clone)]
pub struct SignalConfig {
    /// Signal 1: Fraction of clauses that must have reset x_l to fire (0.0–1.0).
    pub xl_reset_fraction: f64,

    /// Signal 2: Number of consecutive checks without improvement before firing.
    pub stagnation_patience: u32,

    /// Signal 3: Mean α_m threshold — fire when mean exceeds this.
    pub alpha_m_mean_threshold: f64,

    /// Signal 3: Number of consecutive checks where α_m mean increased.
    pub alpha_divergence_patience: u32,

    /// Signal 4: Number of consecutive checks where best assignment is unchanged.
    pub assignment_stability_patience: u32,

    /// Minimum number of checks before any signal can fire.
    /// Prevents premature triggering during early dynamics.
    pub warmup_checks: u32,
}

impl Default for SignalConfig {
    fn default() -> Self {
        SignalConfig {
            xl_reset_fraction: 0.5,
            stagnation_patience: 50,
            alpha_m_mean_threshold: 100.0,
            alpha_divergence_patience: 20,
            assignment_stability_patience: 30,
            warmup_checks: 10,
        }
    }
}

/// Tracks DMM dynamics and detects UNSAT indicator signals.
pub struct UnsatSignalDetector {
    config: SignalConfig,
    total_clauses: usize,

    // Signal 1: x_l reset saturation
    xl_reset_count: usize,

    // Signal 2: C(v) stagnation
    best_unsat_count: usize,
    stagnation_counter: u32,

    // Signal 3: α_m divergence
    prev_alpha_mean: f64,
    alpha_divergence_counter: u32,

    // Signal 4: best assignment stability
    best_assignment: Vec<bool>,
    best_assignment_unsat: usize,
    assignment_unchanged_counter: u32,

    // General
    check_count: u32,
}

impl UnsatSignalDetector {
    /// Create a new detector for a formula with the given number of variables and clauses.
    pub fn new(num_vars: usize, num_clauses: usize, config: SignalConfig) -> Self {
        UnsatSignalDetector {
            config,
            total_clauses: num_clauses,
            xl_reset_count: 0,
            best_unsat_count: num_clauses,
            stagnation_counter: 0,
            prev_alpha_mean: 0.0,
            alpha_divergence_counter: 0,
            best_assignment: vec![false; num_vars],
            best_assignment_unsat: num_clauses,
            assignment_unchanged_counter: 0,
            check_count: 0,
        }
    }

    /// Update the detector with current DMM state. Call at regular intervals
    /// (e.g., every stagnation_check_interval steps).
    ///
    /// Returns `Some(signal)` if a signal fires, `None` otherwise.
    pub fn update(&mut self, state: &DmmState, c_m: &[f64]) -> Option<SignalKind> {
        self.check_count += 1;

        let current_unsat = count_unsat(c_m);
        let current_assignment = extract_assignment(&state.v);

        // --- Signal 1: x_l reset saturation ---
        // Count clauses where x_l has been reset (x_l == 1.0 AND alpha_m == 1.0
        // is the reset signature, but we can't distinguish initial state from reset
        // without explicit tracking). Instead, count clauses at max x_l — these are
        // about to reset, indicating persistent frustration.
        // More robust: count clauses that have alpha_m == 1.0 AND x_l == 1.0 AND
        // we're past warmup (so they must have been reset, not just initialized).
        if self.check_count > self.config.warmup_checks {
            self.xl_reset_count = state
                .x_l
                .iter()
                .zip(state.alpha_m.iter())
                .filter(|(&xl, &am)| xl <= 1.0 + 1e-10 && am <= 1.0 + 1e-10)
                .count();
        }

        // --- Signal 2: C(v) stagnation ---
        if current_unsat < self.best_unsat_count {
            self.best_unsat_count = current_unsat;
            self.stagnation_counter = 0;
        } else {
            self.stagnation_counter += 1;
        }

        // --- Signal 3: α_m divergence ---
        let alpha_mean: f64 =
            state.alpha_m.iter().sum::<f64>() / self.total_clauses.max(1) as f64;
        if alpha_mean > self.prev_alpha_mean + 1e-10 {
            self.alpha_divergence_counter += 1;
        } else {
            self.alpha_divergence_counter = 0;
        }
        self.prev_alpha_mean = alpha_mean;

        // --- Signal 4: best assignment stability ---
        if current_unsat < self.best_assignment_unsat {
            self.best_assignment_unsat = current_unsat;
            self.best_assignment.clone_from_slice(&current_assignment);
            self.assignment_unchanged_counter = 0;
        } else if current_assignment == self.best_assignment {
            self.assignment_unchanged_counter += 1;
        }
        // If assignment changed but didn't improve, don't reset counter

        // --- Check signals (only after warmup) ---
        if self.check_count <= self.config.warmup_checks {
            return None;
        }

        // Signal 1: x_l reset saturation
        let reset_fraction = self.xl_reset_count as f64 / self.total_clauses.max(1) as f64;
        if reset_fraction >= self.config.xl_reset_fraction {
            return Some(SignalKind::XlResetSaturation);
        }

        // Signal 2: C(v) stagnation
        if self.stagnation_counter >= self.config.stagnation_patience {
            return Some(SignalKind::CvStagnation);
        }

        // Signal 3: α_m divergence
        if self.alpha_divergence_counter >= self.config.alpha_divergence_patience
            && alpha_mean >= self.config.alpha_m_mean_threshold
        {
            return Some(SignalKind::AlphaMDivergence);
        }

        // Signal 4: best assignment stability
        if self.assignment_unchanged_counter >= self.config.assignment_stability_patience {
            return Some(SignalKind::BestAssignmentStability);
        }

        None
    }

    /// Get the best assignment seen so far (for CaDiCaL phase seeding).
    pub fn best_assignment(&self) -> &[bool] {
        &self.best_assignment
    }

    /// Get the best unsatisfied clause count seen.
    pub fn best_unsat_count(&self) -> usize {
        self.best_assignment_unsat
    }

    /// Reset the detector for a new restart attempt (keeps config and total_clauses).
    /// Does NOT reset best_assignment — that persists across restarts.
    pub fn reset_for_restart(&mut self) {
        self.xl_reset_count = 0;
        self.stagnation_counter = 0;
        self.prev_alpha_mean = 0.0;
        self.alpha_divergence_counter = 0;
        self.assignment_unchanged_counter = 0;
        self.check_count = 0;
        // Keep best_assignment and best_assignment_unsat across restarts
    }

    /// Get a summary of current signal state for logging.
    pub fn signal_summary(&self) -> SignalSummary {
        SignalSummary {
            xl_reset_fraction: self.xl_reset_count as f64 / self.total_clauses.max(1) as f64,
            stagnation_counter: self.stagnation_counter,
            alpha_m_mean: self.prev_alpha_mean,
            alpha_divergence_counter: self.alpha_divergence_counter,
            assignment_unchanged_counter: self.assignment_unchanged_counter,
            best_unsat: self.best_assignment_unsat,
            check_count: self.check_count,
        }
    }
}

/// Summary of current signal state for logging/diagnostics.
#[derive(Debug)]
pub struct SignalSummary {
    pub xl_reset_fraction: f64,
    pub stagnation_counter: u32,
    pub alpha_m_mean: f64,
    pub alpha_divergence_counter: u32,
    pub assignment_unchanged_counter: u32,
    pub best_unsat: usize,
    pub check_count: u32,
}

impl std::fmt::Display for SignalSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "xl_reset={:.1}% stag={} α_mean={:.1} α_div={} assign_stable={} best_unsat={}",
            self.xl_reset_fraction * 100.0,
            self.stagnation_counter,
            self.alpha_m_mean,
            self.alpha_divergence_counter,
            self.assignment_unchanged_counter,
            self.best_unsat,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dmm::DmmState;
    use crate::formula::Formula;

    fn make_formula_and_state() -> (Formula, DmmState) {
        let f = Formula::new(3, vec![vec![1, -2, 3], vec![-1, 2, -3], vec![1, 2, 3]]);
        let state = DmmState::new(&f, 42);
        (f, state)
    }

    #[test]
    fn test_no_signal_during_warmup() {
        let (f, state) = make_formula_and_state();
        let config = SignalConfig {
            warmup_checks: 5,
            stagnation_patience: 1, // would fire immediately without warmup
            ..Default::default()
        };
        let mut detector = UnsatSignalDetector::new(f.num_vars, f.num_clauses(), config);
        let c_m = vec![0.6, 0.7, 0.8]; // all unsatisfied

        // Should not fire during warmup
        for _ in 0..5 {
            assert_eq!(detector.update(&state, &c_m), None);
        }
    }

    #[test]
    fn test_stagnation_signal_fires() {
        let (f, state) = make_formula_and_state();
        let config = SignalConfig {
            warmup_checks: 2,
            stagnation_patience: 5, // needs 5 stagnant checks to fire
            xl_reset_fraction: 1.0,           // disable signal 1
            alpha_m_mean_threshold: 1e10,      // disable signal 3
            assignment_stability_patience: 100, // disable signal 4
            ..Default::default()
        };
        let mut detector = UnsatSignalDetector::new(f.num_vars, f.num_clauses(), config);
        let c_m = vec![0.6, 0.7, 0.8];

        // Warmup (2 checks) — stagnation counter accumulates to 2
        for _ in 0..2 {
            assert_eq!(detector.update(&state, &c_m), None);
        }

        // Post-warmup: counter is at 2, need 3 more to reach patience=5
        assert_eq!(detector.update(&state, &c_m), None); // counter=3
        assert_eq!(detector.update(&state, &c_m), None); // counter=4
        assert_eq!(
            detector.update(&state, &c_m),
            Some(SignalKind::CvStagnation) // counter=5, fires
        );
    }

    #[test]
    fn test_stagnation_resets_on_improvement() {
        let (f, mut state) = make_formula_and_state();
        let config = SignalConfig {
            warmup_checks: 0,
            stagnation_patience: 3,
            xl_reset_fraction: 1.0,
            alpha_m_mean_threshold: 1e10,
            assignment_stability_patience: 100,
            ..Default::default()
        };
        let mut detector = UnsatSignalDetector::new(f.num_vars, f.num_clauses(), config);

        // 2 checks with 3 unsat
        let c_m_bad = vec![0.6, 0.7, 0.8];
        detector.update(&state, &c_m_bad);
        detector.update(&state, &c_m_bad);

        // Improvement: only 2 unsat
        let c_m_better = vec![0.6, 0.7, 0.3];
        assert_eq!(detector.update(&state, &c_m_better), None);

        // 2 more stagnant checks — still shouldn't fire (counter reset)
        state.v = vec![0.5, -0.5, 0.5]; // change assignment to avoid signal 4
        assert_eq!(detector.update(&state, &c_m_better), None);
        assert_eq!(detector.update(&state, &c_m_better), None);
    }

    #[test]
    fn test_xl_reset_saturation() {
        let (f, mut state) = make_formula_and_state();
        let config = SignalConfig {
            warmup_checks: 1,
            xl_reset_fraction: 0.5,
            stagnation_patience: 100,
            alpha_m_mean_threshold: 1e10,
            assignment_stability_patience: 100,
            ..Default::default()
        };
        let mut detector = UnsatSignalDetector::new(f.num_vars, f.num_clauses(), config);
        let c_m = vec![0.6, 0.7, 0.8];

        // Warmup
        detector.update(&state, &c_m);

        // Simulate 2/3 clauses having been reset (x_l=1, alpha_m=1)
        state.x_l = vec![1.0, 1.0, 50.0];
        state.alpha_m = vec![1.0, 1.0, 10.0];

        let result = detector.update(&state, &c_m);
        assert_eq!(result, Some(SignalKind::XlResetSaturation));
    }

    #[test]
    fn test_alpha_divergence() {
        let (f, mut state) = make_formula_and_state();
        let config = SignalConfig {
            warmup_checks: 0,
            alpha_divergence_patience: 4, // need 4 consecutive increases
            alpha_m_mean_threshold: 10.0,
            xl_reset_fraction: 1.0,
            stagnation_patience: 100,
            assignment_stability_patience: 100,
            ..Default::default()
        };
        let mut detector = UnsatSignalDetector::new(f.num_vars, f.num_clauses(), config);
        let c_m = vec![0.6, 0.7, 0.8];

        // Simulate increasing alpha_m over multiple checks
        // Check 0: prev_alpha_mean=0, new mean=11 → divergence_counter=1
        // Check 1: prev=11, new=16 → counter=2
        // Check 2: prev=16, new=21 → counter=3
        // Check 3: prev=21, new=26 → counter=4 → fires (mean 26 > threshold 10)
        for i in 0..4 {
            let base = 10.0 + (i as f64) * 5.0;
            state.alpha_m = vec![base, base + 1.0, base + 2.0];
            let result = detector.update(&state, &c_m);
            if i < 3 {
                assert_eq!(result, None, "Should not fire at check {}", i);
            } else {
                assert_eq!(result, Some(SignalKind::AlphaMDivergence));
            }
        }
    }

    #[test]
    fn test_reset_for_restart() {
        let (f, state) = make_formula_and_state();
        let config = SignalConfig::default();
        let mut detector = UnsatSignalDetector::new(f.num_vars, f.num_clauses(), config);
        let c_m = vec![0.3, 0.7, 0.8]; // 2 unsat

        detector.update(&state, &c_m);
        assert_eq!(detector.best_unsat_count(), 2);

        detector.reset_for_restart();
        assert_eq!(detector.best_unsat_count(), 2); // preserved across restart
        assert_eq!(detector.signal_summary().check_count, 0); // counters reset
    }

    #[test]
    fn test_signal_summary_display() {
        let (f, state) = make_formula_and_state();
        let config = SignalConfig::default();
        let mut detector = UnsatSignalDetector::new(f.num_vars, f.num_clauses(), config);
        let c_m = vec![0.6, 0.7, 0.8];
        detector.update(&state, &c_m);

        let summary = detector.signal_summary();
        let display = format!("{}", summary);
        assert!(display.contains("xl_reset="));
        assert!(display.contains("stag="));
        assert!(display.contains("best_unsat="));
    }
}
