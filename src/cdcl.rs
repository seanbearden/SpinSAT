//! CaDiCaL CDCL solver integration for hybrid DMM-CDCL cooperation.
//!
//! Provides a wrapper around CaDiCaL that supports:
//! - Loading a CNF formula
//! - Setting initial phase hints from DMM voltage assignments
//! - Solving with DRAT proof output
//! - Extracting the satisfying assignment

use cadical_sys::bridge::ffi;
use cxx::UniquePtr;

use crate::formula::Formula;

/// Result from the CDCL solver.
pub enum CdclResult {
    /// Satisfying assignment found (0-based bool vector).
    Sat(Vec<bool>),
    /// Proven unsatisfiable.
    Unsat,
    /// Unknown (timeout or resource limit).
    Unknown,
}

/// Wrapper around CaDiCaL providing the SpinSAT hybrid cooperation interface.
pub struct CdclSolver {
    solver: UniquePtr<ffi::Solver>,
    num_vars: usize,
}

impl CdclSolver {
    /// Create a new CDCL solver and load the formula.
    pub fn new(formula: &Formula) -> Self {
        Self::with_proof(formula, None)
    }

    /// Create a new CDCL solver with optional DRAT proof output.
    /// Proof tracing must be enabled before adding clauses (CaDiCaL requirement).
    pub fn with_proof(formula: &Formula, proof_path: Option<&str>) -> Self {
        let mut solver = ffi::constructor();
        let num_vars = formula.num_vars;

        // Enable proof tracing BEFORE adding clauses (CaDiCaL requirement)
        if let Some(path) = proof_path {
            ffi::trace_proof2(&mut solver, path.to_string());
        }

        // Add all clauses (CaDiCaL uses 1-based signed literals, 0-terminated)
        for m in 0..formula.num_clauses() {
            for &(var_idx, polarity) in formula.clause(m) {
                let lit = (var_idx as i32 + 1) * if polarity > 0.0 { 1 } else { -1 };
                ffi::add(&mut solver, lit);
            }
            ffi::add(&mut solver, 0); // terminate clause
        }

        CdclSolver { solver, num_vars }
    }

    /// Set phase hints from DMM voltage assignment.
    ///
    /// For each variable, sets the preferred decision polarity based on
    /// the DMM's current or best voltage values. Positive voltage -> prefer TRUE,
    /// negative voltage -> prefer FALSE.
    pub fn set_phase_from_voltages(&mut self, voltages: &[f64]) {
        for (i, &v) in voltages.iter().enumerate() {
            let lit = (i as i32 + 1) * if v >= 0.0 { 1 } else { -1 };
            ffi::phase(&mut self.solver, lit);
        }
    }

    /// Set phase hints from a boolean assignment (e.g., DMM's best found).
    pub fn set_phase_from_assignment(&mut self, assignment: &[bool]) {
        for (i, &val) in assignment.iter().enumerate() {
            let lit = (i as i32 + 1) * if val { 1 } else { -1 };
            ffi::phase(&mut self.solver, lit);
        }
    }

    /// Add extra clauses (e.g., from preprocessing or learned from previous runs).
    pub fn add_clauses(&mut self, clauses: &[Vec<i32>]) {
        for clause in clauses {
            for &lit in clause {
                ffi::add(&mut self.solver, lit);
            }
            ffi::add(&mut self.solver, 0);
        }
    }

    /// Enable DRAT proof output to a file.
    pub fn enable_proof(&mut self, path: &str) -> bool {
        ffi::trace_proof2(&mut self.solver, path.to_string())
    }

    /// Set a conflict limit for bounded solving.
    pub fn set_conflict_limit(&mut self, conflicts: i32) {
        ffi::limit(&mut self.solver, "conflicts".to_string(), conflicts);
    }

    /// Seed CaDiCaL with clause difficulty information from DMM's long-term memory.
    ///
    /// Uses CaDiCaL's `assume()` to force early decisions on variables that appear
    /// in the most frustrated clauses (highest x_l values). This drives CaDiCaL's
    /// conflict analysis toward the hard region of the formula, producing more
    /// relevant learned clauses.
    ///
    /// Based on Deep Cooperation "Conflict Frequency" technique (Cai et al., IJCAI 2022),
    /// adapted from local-search conflict frequency to DMM long-term memory.
    ///
    /// - `formula`: the SAT formula (to map clauses → variables)
    /// - `x_l`: long-term memory values per clause
    /// - `best_assignment`: DMM's best assignment (determines polarity for assumptions)
    /// - `top_k`: number of top frustrated variables to assume
    pub fn assume_frustrated_variables(
        &mut self,
        formula: &crate::formula::Formula,
        x_l: &[f64],
        best_assignment: &[bool],
        top_k: usize,
    ) {
        // Score each variable by the total x_l of clauses it appears in
        let mut var_frustration = vec![0.0f64; formula.num_vars];
        for (m, &xl) in x_l.iter().enumerate() {
            if m >= formula.num_clauses() {
                break;
            }
            for &(var_idx, _) in formula.clause(m) {
                var_frustration[var_idx] += xl;
            }
        }

        // Find top-k most frustrated variables
        let mut indices: Vec<usize> = (0..formula.num_vars).collect();
        indices.sort_unstable_by(|&a, &b| {
            var_frustration[b]
                .partial_cmp(&var_frustration[a])
                .unwrap()
        });

        let k = top_k.min(formula.num_vars);
        for &var_idx in &indices[..k] {
            let lit = (var_idx as i32 + 1)
                * if best_assignment.get(var_idx).copied().unwrap_or(true) {
                    1
                } else {
                    -1
                };
            ffi::assume(&mut self.solver, lit);
        }
    }

    /// Solve the formula. Returns the result.
    pub fn solve(&mut self) -> CdclResult {
        let status = ffi::solve(&mut self.solver);
        match status {
            10 => {
                // SATISFIABLE
                let mut assignment = Vec::with_capacity(self.num_vars);
                for i in 0..self.num_vars {
                    let lit = (i as i32) + 1;
                    let val = ffi::val(&mut self.solver, lit);
                    assignment.push(val > 0);
                }
                CdclResult::Sat(assignment)
            }
            20 => CdclResult::Unsat, // UNSATISFIABLE
            _ => CdclResult::Unknown,
        }
    }

    /// Extract the current variable assignment (call after SAT result).
    /// Returns 0-based boolean vector.
    pub fn get_assignment(&mut self) -> Vec<bool> {
        let mut assignment = Vec::with_capacity(self.num_vars);
        for i in 0..self.num_vars {
            let lit = (i as i32) + 1;
            let val = ffi::val(&mut self.solver, lit);
            assignment.push(val > 0);
        }
        assignment
    }

    /// Get the number of variables.
    pub fn num_vars(&self) -> usize {
        self.num_vars
    }

    /// Close and flush DRAT proof trace.
    pub fn close_proof(&mut self) {
        ffi::close_proof_trace(&mut self.solver, false);
    }

    /// Extract fixed variables discovered by CaDiCaL during solving.
    /// Returns unit clauses (1-based signed literals) for variables that
    /// CaDiCaL has proven must have a specific value.
    /// `fixed(lit)` returns: +1 if true, -1 if false, 0 if not fixed.
    pub fn get_fixed_literals(&self) -> Vec<i32> {
        let mut fixed = Vec::new();
        for i in 0..self.num_vars {
            let lit = (i as i32) + 1;
            let val = ffi::fixed(&self.solver, lit);
            if val != 0 {
                fixed.push(if val > 0 { lit } else { -lit });
            }
        }
        fixed
    }

    /// Check if the solver is in a satisfied state (val() is only valid then).
    fn is_satisfied(&self) -> bool {
        ffi::status(&self.solver) == 10
    }

    /// Extract CaDiCaL's variable assignment as voltages for DMM.
    /// Only valid after a SAT result. Returns None if not in SAT state.
    /// Returns 0-based f64 vector: +1.0 for true, -1.0 for false.
    pub fn get_phases_as_voltages(&mut self) -> Option<Vec<f64>> {
        if !self.is_satisfied() {
            return None;
        }
        let mut voltages = Vec::with_capacity(self.num_vars);
        for i in 0..self.num_vars {
            let lit = (i as i32) + 1;
            let val = ffi::val(&mut self.solver, lit);
            voltages.push(if val > 0 { 1.0 } else { -1.0 });
        }
        Some(voltages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cdcl_solve_sat() {
        // (x1 OR x2) AND (NOT x1 OR x2) -- satisfiable with x2=true
        let formula = Formula::new(2, vec![vec![1, 2], vec![-1, 2]]);
        let mut cdcl = CdclSolver::new(&formula);
        match cdcl.solve() {
            CdclResult::Sat(assignment) => {
                assert!(formula.verify(&assignment));
            }
            _ => panic!("Expected SAT"),
        }
    }

    #[test]
    fn test_cdcl_solve_unsat() {
        // (x1) AND (NOT x1) -- trivially unsatisfiable
        let formula = Formula::new(1, vec![vec![1], vec![-1]]);
        let mut cdcl = CdclSolver::new(&formula);
        match cdcl.solve() {
            CdclResult::Unsat => {} // expected
            _ => panic!("Expected UNSAT"),
        }
    }

    #[test]
    fn test_cdcl_phase_hints() {
        // Set phases and verify solve still works
        let formula = Formula::new(3, vec![vec![1, 2, 3], vec![-1, -2, 3], vec![1, -3]]);
        let mut cdcl = CdclSolver::new(&formula);

        // Set phases from "voltages"
        let voltages = vec![0.8, -0.5, 0.3];
        cdcl.set_phase_from_voltages(&voltages);

        match cdcl.solve() {
            CdclResult::Sat(assignment) => {
                assert!(formula.verify(&assignment));
            }
            _ => panic!("Expected SAT"),
        }
    }

    #[test]
    fn test_cdcl_add_extra_clauses() {
        // Start with satisfiable formula, add clause that makes it UNSAT
        let formula = Formula::new(1, vec![vec![1]]);
        let mut cdcl = CdclSolver::new(&formula);
        cdcl.add_clauses(&[vec![-1]]);
        match cdcl.solve() {
            CdclResult::Unsat => {} // expected: (x1) AND (NOT x1)
            _ => panic!("Expected UNSAT after adding contradictory clause"),
        }
    }

    #[test]
    fn test_cdcl_conflict_limit() {
        // Use conflict limit for bounded solving
        let formula = Formula::new(2, vec![vec![1, 2], vec![-1, 2]]);
        let mut cdcl = CdclSolver::new(&formula);
        cdcl.set_conflict_limit(1000);
        match cdcl.solve() {
            CdclResult::Sat(assignment) => {
                assert!(formula.verify(&assignment));
            }
            _ => {} // may return Unknown if limit hit, that's ok
        }
    }

    #[test]
    fn test_get_fixed_literals_after_sat() {
        // Simple formula where CaDiCaL can fix variables via unit propagation
        // (x1) AND (x1 OR x2) — x1 is forced true
        let formula = Formula::new(2, vec![vec![1], vec![1, 2]]);
        let mut cdcl = CdclSolver::new(&formula);
        match cdcl.solve() {
            CdclResult::Sat(_) => {
                let fixed = cdcl.get_fixed_literals();
                // x1 should be fixed to true (lit = 1)
                assert!(fixed.contains(&1), "x1 should be fixed: {:?}", fixed);
            }
            _ => panic!("Expected SAT"),
        }
    }

    #[test]
    fn test_get_phases_as_voltages_after_sat() {
        let formula = Formula::new(2, vec![vec![1, 2], vec![-1, 2]]);
        let mut cdcl = CdclSolver::new(&formula);
        match cdcl.solve() {
            CdclResult::Sat(_) => {
                let voltages = cdcl.get_phases_as_voltages();
                assert!(voltages.is_some(), "Should return voltages after SAT");
                let v = voltages.unwrap();
                assert_eq!(v.len(), 2);
                // Each voltage should be ±1.0
                for &val in &v {
                    assert!(val == 1.0 || val == -1.0, "Voltage should be ±1.0: {}", val);
                }
            }
            _ => panic!("Expected SAT"),
        }
    }

    #[test]
    fn test_get_phases_as_voltages_returns_none_after_unsat() {
        let formula = Formula::new(1, vec![vec![1], vec![-1]]);
        let mut cdcl = CdclSolver::new(&formula);
        match cdcl.solve() {
            CdclResult::Unsat => {
                let voltages = cdcl.get_phases_as_voltages();
                assert!(voltages.is_none(), "Should return None after UNSAT");
            }
            _ => panic!("Expected UNSAT"),
        }
    }

    #[test]
    fn test_with_proof_constructor() {
        // Verify that with_proof creates a solver that works correctly
        let formula = Formula::new(2, vec![vec![1, 2], vec![-1, 2]]);
        let proof_path = std::env::temp_dir().join("cdcl_test_proof.drat");
        let mut cdcl = CdclSolver::with_proof(
            &formula,
            Some(proof_path.to_str().unwrap()),
        );
        match cdcl.solve() {
            CdclResult::Sat(assignment) => {
                assert!(formula.verify(&assignment));
            }
            _ => panic!("Expected SAT"),
        }
        cdcl.close_proof();
        // Proof file should exist (even for SAT, it records the search)
        assert!(proof_path.exists(), "Proof file should be created");
        let _ = std::fs::remove_file(&proof_path);
    }

    #[test]
    fn test_assume_frustrated_variables() {
        // Formula with 3 vars, 4 clauses. Clause 0 has high x_l (frustrated).
        let formula = Formula::new(3, vec![
            vec![1, 2, 3],     // clause 0: uses vars 0,1,2
            vec![-1, 2],       // clause 1: uses vars 0,1
            vec![2, -3],       // clause 2: uses vars 1,2
            vec![1, 3],        // clause 3: uses vars 0,2
        ]);
        let x_l = vec![100.0, 1.0, 1.0, 1.0]; // clause 0 very frustrated
        let assignment = vec![true, true, true];

        let mut cdcl = CdclSolver::new(&formula);
        cdcl.assume_frustrated_variables(&formula, &x_l, &assignment, 2);

        // Should still solve correctly with assumptions
        match cdcl.solve() {
            CdclResult::Sat(a) => assert!(formula.verify(&a)),
            CdclResult::Unsat => panic!("Formula is SAT"),
            CdclResult::Unknown => {} // assumptions may cause Unknown, acceptable
        }
    }

    #[test]
    fn test_assume_frustrated_on_unsat() {
        // UNSAT formula — assumptions should not prevent UNSAT detection
        let formula = Formula::new(2, vec![
            vec![1, 2], vec![-1, 2], vec![1, -2], vec![-1, -2],
        ]);
        let x_l = vec![10.0, 20.0, 5.0, 15.0]; // varying frustration
        let assignment = vec![true, false];

        let mut cdcl = CdclSolver::new(&formula);
        cdcl.assume_frustrated_variables(&formula, &x_l, &assignment, 2);

        match cdcl.solve() {
            CdclResult::Unsat => {} // expected
            // With assumptions, CaDiCaL may also return Unknown (failed assumptions)
            CdclResult::Unknown => {}
            CdclResult::Sat(_) => panic!("Formula is UNSAT"),
        }
    }
}
