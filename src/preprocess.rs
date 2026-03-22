//! CNF preprocessing pipeline to reduce formula size before ODE integration.
//!
//! Each eliminated variable and clause saves continuous integration cost across
//! all timesteps, so preprocessing provides outsized benefit for DMM solvers.

use std::collections::{HashMap, HashSet};

/// Result of preprocessing: a reduced formula plus the mapping to recover
/// the full assignment from the reduced solution.
pub struct PreprocessResult {
    /// Reduced clauses (1-based signed literals, original variable numbering replaced).
    pub clauses: Vec<Vec<i32>>,
    /// Number of variables in the reduced formula.
    pub num_vars: usize,
    /// Maps reduced variable index (1-based) → original variable index (1-based).
    pub var_map: Vec<usize>,
    /// Variables fixed during preprocessing: (original_var_1based, value).
    pub fixed_vars: Vec<(usize, bool)>,
    /// BVE elimination stack: (eliminated_var_1based, original_clauses_containing_var).
    /// Processed in reverse order during reconstruction.
    pub bve_stack: Vec<BveElimination>,
    /// Statistics from preprocessing.
    pub stats: PreprocessStats,
}

/// Records a BVE elimination for correct variable reconstruction.
pub struct BveElimination {
    /// The eliminated variable (1-based).
    pub var: usize,
    /// The original clauses that contained this variable (before resolution).
    /// Used to find a satisfying value during reconstruction.
    pub original_clauses: Vec<Vec<i32>>,
}

impl PreprocessResult {
    /// Reconstruct full assignment from the reduced solution.
    /// `reduced_assignment` is indexed by reduced variable (0-based).
    ///
    /// BVE-eliminated variables are reconstructed by trying to satisfy their
    /// original clauses, processed in reverse elimination order.
    pub fn reconstruct_assignment(&self, reduced_assignment: &[bool], original_num_vars: usize) -> Vec<bool> {
        let mut full = vec![false; original_num_vars];

        // Apply fixed variables (unit prop, pure literal, failed literal)
        for &(orig_var, val) in &self.fixed_vars {
            full[orig_var - 1] = val;
        }

        // Map reduced solution back to original variables
        for (reduced_idx, &val) in reduced_assignment.iter().enumerate() {
            let orig_var = self.var_map[reduced_idx];
            full[orig_var - 1] = val;
        }

        // Reconstruct BVE-eliminated variables in reverse order.
        // For each eliminated var, find a value that satisfies all its original clauses
        // given the current assignment of other variables.
        for elim in self.bve_stack.iter().rev() {
            let var_idx = elim.var - 1; // 0-based
            // Try true first, then false
            full[var_idx] = true;
            if self.all_clauses_satisfied(&elim.original_clauses, &full) {
                continue;
            }
            full[var_idx] = false;
            // If false doesn't work either, one of {true, false} must work since
            // BVE preserves equisatisfiability. If neither works, the reduced formula
            // was UNSAT (shouldn't happen if solver returned SAT). Leave as false.
        }

        full
    }

    /// Check if all clauses are satisfied by the given assignment.
    fn all_clauses_satisfied(&self, clauses: &[Vec<i32>], assignment: &[bool]) -> bool {
        clauses.iter().all(|clause| {
            clause.iter().any(|&lit| {
                let var_idx = (lit.unsigned_abs() as usize) - 1;
                let val = assignment[var_idx];
                (lit > 0 && val) || (lit < 0 && !val)
            })
        })
    }
}

#[derive(Default, Debug)]
pub struct PreprocessStats {
    pub vars_eliminated: usize,
    pub clauses_eliminated: usize,
    pub unit_props: usize,
    pub pure_literals: usize,
    pub subsumptions: usize,
    pub self_subsumptions: usize,
    pub bve_eliminations: usize,
    pub failed_literals: usize,
}

/// Run the full preprocessing pipeline on raw DIMACS clauses.
/// `num_vars` is from the DIMACS header; `clauses` are signed literal vectors.
///
/// Preprocessing cost scales with clause count. For very large instances
/// (>100K clauses), only cheap O(n) passes run; expensive O(n²) passes
/// are gated by per-technique thresholds.
pub fn preprocess(num_vars: usize, clauses: Vec<Vec<i32>>) -> PreprocessResult {
    /// Skip ALL preprocessing for extremely large instances where even
    /// unit propagation is too slow (assign_var is O(clauses) per assignment).
    const MAX_CLAUSE_LITERAL_PRODUCT: usize = 50_000_000;

    let total_literals: usize = clauses.iter().map(|c| c.len()).sum();
    if clauses.len().saturating_mul(total_literals) > MAX_CLAUSE_LITERAL_PRODUCT {
        // Formula too large for linear-scan preprocessing. Return as-is.
        let var_map: Vec<usize> = (1..=num_vars).collect();
        return PreprocessResult {
            clauses,
            num_vars,
            var_map,
            fixed_vars: Vec::new(),
            bve_stack: Vec::new(),
            stats: PreprocessStats::default(),
        };
    }

    let mut state = PreprocessState::new(num_vars, clauses);

    // Phase 1: Core reductions (iterate until fixpoint)
    loop {
        let changed1 = state.unit_propagate();
        let changed2 = state.pure_literal_eliminate();
        if !changed1 && !changed2 {
            break;
        }
    }

    // Phase 2: Clause simplification
    state.subsumption_eliminate();
    state.self_subsuming_resolve();

    // Re-run Phase 1 since Phase 2 may create new unit/pure literals
    loop {
        let changed1 = state.unit_propagate();
        let changed2 = state.pure_literal_eliminate();
        if !changed1 && !changed2 {
            break;
        }
    }

    // Phase 3: Variable elimination
    state.bounded_variable_eliminate();

    // Final fixpoint of core reductions
    loop {
        let changed1 = state.unit_propagate();
        let changed2 = state.pure_literal_eliminate();
        if !changed1 && !changed2 {
            break;
        }
    }

    // Phase 4: Failed literal probing (expensive, run once)
    state.failed_literal_probe();

    // Final cleanup
    loop {
        let changed1 = state.unit_propagate();
        let changed2 = state.pure_literal_eliminate();
        if !changed1 && !changed2 {
            break;
        }
    }

    state.finalize()
}

/// Internal mutable state for preprocessing.
struct PreprocessState {
    num_vars: usize,
    /// Active clauses (None = deleted).
    clauses: Vec<Option<Vec<i32>>>,
    /// Variables assigned during preprocessing: var (1-based) → value.
    assigned: HashMap<usize, bool>,
    /// BVE elimination stack for correct variable reconstruction.
    bve_stack: Vec<BveElimination>,
    stats: PreprocessStats,
}

impl PreprocessState {
    fn new(num_vars: usize, clauses: Vec<Vec<i32>>) -> Self {
        let clauses = clauses.into_iter().map(Some).collect();
        PreprocessState {
            num_vars,
            clauses,
            assigned: HashMap::new(),
            bve_stack: Vec::new(),
            stats: PreprocessStats::default(),
        }
    }

    /// Assign a variable and propagate: remove satisfied clauses, shorten others.
    fn assign_var(&mut self, var: usize, value: bool) {
        self.assigned.insert(var, value);
        let true_lit = if value { var as i32 } else { -(var as i32) };
        let false_lit = -true_lit;

        for clause in self.clauses.iter_mut() {
            if let Some(lits) = clause {
                if lits.contains(&true_lit) {
                    // Clause satisfied — remove it
                    *clause = None;
                    self.stats.clauses_eliminated += 1;
                } else {
                    // Remove the false literal
                    lits.retain(|&l| l != false_lit);
                }
            }
        }
        self.stats.vars_eliminated += 1;
    }

    /// Unit propagation: find single-literal clauses, assign them, cascade.
    /// Returns true if any assignment was made.
    fn unit_propagate(&mut self) -> bool {
        let mut changed = false;
        loop {
            let mut unit = None;
            for clause in self.clauses.iter() {
                if let Some(lits) = clause {
                    if lits.len() == 1 {
                        let lit = lits[0];
                        let var = lit.unsigned_abs() as usize;
                        if !self.assigned.contains_key(&var) {
                            unit = Some((var, lit > 0));
                            break;
                        }
                    } else if lits.is_empty() {
                        // Empty clause = UNSAT. We can't handle this in DMM preprocessing,
                        // so just stop propagating.
                        return changed;
                    }
                }
            }

            match unit {
                Some((var, val)) => {
                    self.assign_var(var, val);
                    self.stats.unit_props += 1;
                    changed = true;
                }
                None => break,
            }
        }
        changed
    }

    /// Pure literal elimination: if a variable appears in only one polarity,
    /// assign it to satisfy all those clauses.
    /// Returns true if any assignment was made.
    fn pure_literal_eliminate(&mut self) -> bool {
        let mut pos = HashSet::new();
        let mut neg = HashSet::new();

        for clause in self.clauses.iter() {
            if let Some(lits) = clause {
                for &lit in lits {
                    let var = lit.unsigned_abs() as usize;
                    if self.assigned.contains_key(&var) {
                        continue;
                    }
                    if lit > 0 {
                        pos.insert(var);
                    } else {
                        neg.insert(var);
                    }
                }
            }
        }

        let mut changed = false;

        // Variables in pos but not neg → assign true
        for &var in pos.iter() {
            if !neg.contains(&var) && !self.assigned.contains_key(&var) {
                self.assign_var(var, true);
                self.stats.pure_literals += 1;
                changed = true;
            }
        }

        // Variables in neg but not pos → assign false
        for &var in neg.iter() {
            if !pos.contains(&var) && !self.assigned.contains_key(&var) {
                self.assign_var(var, false);
                self.stats.pure_literals += 1;
                changed = true;
            }
        }

        changed
    }

    /// Subsumption elimination: if clause A is a subset of clause B, remove B.
    /// Skipped when active clause count exceeds threshold (O(n²) worst case).
    fn subsumption_eliminate(&mut self) {
        const SUBSUMP_MAX_CLAUSES: usize = 50_000;

        let n = self.clauses.len();

        // Collect active clauses sorted by length (shorter first = more likely to subsume)
        let mut indices: Vec<usize> = (0..n)
            .filter(|&i| self.clauses[i].is_some())
            .collect();

        if indices.len() > SUBSUMP_MAX_CLAUSES {
            return; // Skip — too many clauses for O(n²) pairwise check
        }

        indices.sort_by_key(|&i| self.clauses[i].as_ref().unwrap().len());

        // For each pair, check if shorter subsumes longer
        let mut removed = HashSet::new();
        for i in 0..indices.len() {
            let idx_a = indices[i];
            if removed.contains(&idx_a) {
                continue;
            }
            let clause_a: Vec<i32> = self.clauses[idx_a].as_ref().unwrap().clone();
            let set_a: HashSet<i32> = clause_a.iter().copied().collect();

            for j in (i + 1)..indices.len() {
                let idx_b = indices[j];
                if removed.contains(&idx_b) {
                    continue;
                }
                let clause_b = self.clauses[idx_b].as_ref().unwrap();

                // A can only subsume B if |A| <= |B|
                if clause_a.len() > clause_b.len() {
                    continue;
                }

                if set_a.iter().all(|lit| clause_b.contains(lit)) {
                    self.clauses[idx_b] = None;
                    self.stats.subsumptions += 1;
                    self.stats.clauses_eliminated += 1;
                    removed.insert(idx_b);
                }
            }
        }
    }

    /// Self-subsuming resolution: if resolving clause A with clause B produces
    /// a clause that subsumes B, strengthen B by removing the resolved literal.
    /// Skipped when active clause count exceeds threshold (O(n²) per iteration).
    fn self_subsuming_resolve(&mut self) {
        const SELF_SUB_MAX_CLAUSES: usize = 50_000;

        let n = self.clauses.len();
        let active = self.clauses.iter().filter(|c| c.is_some()).count();
        if active > SELF_SUB_MAX_CLAUSES {
            return;
        }

        let mut changed = true;

        while changed {
            changed = false;
            for i in 0..n {
                let clause_a = match &self.clauses[i] {
                    Some(c) => c.clone(),
                    None => continue,
                };

                for j in 0..n {
                    if i == j {
                        continue;
                    }
                    let clause_b = match &self.clauses[j] {
                        Some(c) => c.clone(),
                        None => continue,
                    };

                    // Find if there's exactly one literal l in A where -l is in B,
                    // and all other literals of A are in B.
                    if let Some(resolved_lit) = self.find_self_subsumption(&clause_a, &clause_b) {
                        // Strengthen B by removing -resolved_lit
                        if let Some(ref mut lits) = self.clauses[j] {
                            lits.retain(|&l| l != -resolved_lit);
                            self.stats.self_subsumptions += 1;
                            changed = true;
                        }
                    }
                }
            }
        }
    }

    /// Check if resolving A with B on some literal produces a resolvent that subsumes B.
    /// Returns the literal from A to resolve on, if self-subsumption applies.
    fn find_self_subsumption(&self, clause_a: &[i32], clause_b: &[i32]) -> Option<i32> {
        // For self-subsuming resolution: all lits of A must appear in B except exactly one,
        // and that one's negation must appear in B.
        let mut mismatches = 0;
        let mut resolved_lit = 0;

        for &lit_a in clause_a {
            if clause_b.contains(&lit_a) {
                // Good, literal matches
            } else if clause_b.contains(&(-lit_a)) {
                // This literal resolves
                mismatches += 1;
                resolved_lit = lit_a;
                if mismatches > 1 {
                    return None;
                }
            } else {
                // Literal not in B at all
                return None;
            }
        }

        if mismatches == 1 {
            Some(resolved_lit)
        } else {
            None
        }
    }

    /// Bounded Variable Elimination (BVE): resolve out variables where the
    /// number of resolvents doesn't exceed the number of removed clauses.
    ///
    /// Skips variables where `pos_count * neg_count > BVE_RESOLVENT_CAP` to
    /// avoid O(n²) blowup on high-occurrence variables (e.g., in structured
    /// instances with 500K+ clauses).
    fn bounded_variable_eliminate(&mut self) {
        // Cap: skip BVE candidates where resolution would generate too many pairs
        const BVE_RESOLVENT_CAP: usize = 1000;

        // Build occurrence counts in one pass instead of per-variable scans
        let mut pos_count: Vec<usize> = vec![0; self.num_vars + 1];
        let mut neg_count: Vec<usize> = vec![0; self.num_vars + 1];
        for clause in &self.clauses {
            if let Some(lits) = clause {
                for &lit in lits {
                    let var = lit.unsigned_abs() as usize;
                    if lit > 0 {
                        pos_count[var] += 1;
                    } else {
                        neg_count[var] += 1;
                    }
                }
            }
        }

        let mut candidates: Vec<usize> = (1..=self.num_vars)
            .filter(|&v| !self.assigned.contains_key(&v))
            .filter(|&v| {
                let product = pos_count[v].saturating_mul(neg_count[v]);
                product > 0 && product <= BVE_RESOLVENT_CAP
            })
            .collect();

        // Sort by occurrence count (least-occurring first = most likely to reduce)
        candidates.sort_by_key(|&var| pos_count[var] + neg_count[var]);

        for var in candidates {
            if self.assigned.contains_key(&var) {
                continue;
            }

            let pos_lit = var as i32;
            let neg_lit = -(var as i32);

            // Collect clause indices containing this variable
            let pos_clauses: Vec<usize> = self.clauses.iter().enumerate()
                .filter_map(|(i, c)| {
                    c.as_ref().and_then(|lits| {
                        if lits.contains(&pos_lit) { Some(i) } else { None }
                    })
                })
                .collect();

            let neg_clauses: Vec<usize> = self.clauses.iter().enumerate()
                .filter_map(|(i, c)| {
                    c.as_ref().and_then(|lits| {
                        if lits.contains(&neg_lit) { Some(i) } else { None }
                    })
                })
                .collect();

            if pos_clauses.is_empty() || neg_clauses.is_empty() {
                // Pure literal — should have been caught already, but handle anyway
                continue;
            }

            // Generate all resolvents
            let mut resolvents: Vec<Vec<i32>> = Vec::new();
            let mut _tautology_count = 0;

            for &pi in &pos_clauses {
                let pc = self.clauses[pi].as_ref().unwrap();
                for &ni in &neg_clauses {
                    let nc = self.clauses[ni].as_ref().unwrap();

                    // Resolve: combine both clauses, remove var and -var
                    let mut resolvent: Vec<i32> = Vec::new();
                    let mut is_tautology = false;

                    for &lit in pc.iter().chain(nc.iter()) {
                        let v = lit.unsigned_abs() as usize;
                        if v == var {
                            continue;
                        }
                        if resolvent.contains(&(-lit)) {
                            is_tautology = true;
                            break;
                        }
                        if !resolvent.contains(&lit) {
                            resolvent.push(lit);
                        }
                    }

                    if is_tautology {
                        _tautology_count += 1;
                    } else {
                        resolvents.push(resolvent);
                    }
                }
            }

            let removed = pos_clauses.len() + neg_clauses.len();
            let added = resolvents.len();

            // Only eliminate if we don't increase clause count
            if added <= removed {
                // Save original clauses for reconstruction before removing them
                let mut original_clauses = Vec::new();
                for &idx in pos_clauses.iter().chain(neg_clauses.iter()) {
                    original_clauses.push(self.clauses[idx].as_ref().unwrap().clone());
                }
                self.bve_stack.push(BveElimination {
                    var,
                    original_clauses,
                });

                // Remove original clauses
                for &idx in pos_clauses.iter().chain(neg_clauses.iter()) {
                    self.clauses[idx] = None;
                    self.stats.clauses_eliminated += 1;
                }

                // Add resolvents
                for r in resolvents {
                    self.clauses.push(Some(r));
                }

                self.stats.bve_eliminations += 1;
                self.stats.vars_eliminated += 1;
                // Mark as eliminated (actual value determined during reconstruction)
                self.assigned.insert(var, false);
            }
        }
    }

    fn count_occurrences(&self, var: usize) -> usize {
        let lit_pos = var as i32;
        let lit_neg = -(var as i32);
        self.clauses.iter()
            .filter_map(|c| c.as_ref())
            .filter(|lits| lits.contains(&lit_pos) || lits.contains(&lit_neg))
            .count()
    }

    /// Failed literal probing: for each unassigned variable, tentatively assign
    /// it and run unit propagation. If both polarities lead to the same forced
    /// assignment of another variable, that assignment is implied.
    /// If one polarity leads to a conflict, the other polarity is forced.
    ///
    /// Skipped when clause count exceeds threshold — each probe runs unit
    /// propagation on a copy, so cost is O(vars × clauses).
    fn failed_literal_probe(&mut self) {
        const PROBE_MAX_CLAUSES: usize = 50_000;

        let active_clauses = self.clauses.iter().filter(|c| c.is_some()).count();
        if active_clauses > PROBE_MAX_CLAUSES {
            return;
        }

        let unassigned: Vec<usize> = (1..=self.num_vars)
            .filter(|v| !self.assigned.contains_key(v))
            .collect();

        for var in unassigned {
            if self.assigned.contains_key(&var) {
                continue; // May have been assigned by a previous probe
            }

            // Try positive assignment
            let pos_result = self.probe_assignment(var, true);
            // Try negative assignment
            let neg_result = self.probe_assignment(var, false);

            match (pos_result, neg_result) {
                (ProbeResult::Conflict, ProbeResult::Conflict) => {
                    // Both polarities conflict — formula is UNSAT.
                    // Can't do anything useful here for DMM, skip.
                }
                (ProbeResult::Conflict, _) => {
                    // Positive assignment fails → variable must be false
                    self.assign_var(var, false);
                    self.stats.failed_literals += 1;
                }
                (_, ProbeResult::Conflict) => {
                    // Negative assignment fails → variable must be true
                    self.assign_var(var, true);
                    self.stats.failed_literals += 1;
                }
                (ProbeResult::Forced(pos_implied), ProbeResult::Forced(neg_implied)) => {
                    // Find common forced assignments
                    for (&ivar, &ival) in &pos_implied {
                        if neg_implied.get(&ivar) == Some(&ival) {
                            if !self.assigned.contains_key(&ivar) {
                                self.assign_var(ivar, ival);
                                self.stats.failed_literals += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Simulate assigning a variable and running unit propagation on a snapshot.
    fn probe_assignment(&self, var: usize, value: bool) -> ProbeResult {
        // Clone current clauses for simulation
        let mut sim_clauses: Vec<Option<Vec<i32>>> = self.clauses.clone();
        let mut sim_assigned: HashMap<usize, bool> = HashMap::new();
        let mut queue: Vec<(usize, bool)> = vec![(var, value)];

        while let Some((v, val)) = queue.pop() {
            if let Some(&existing) = sim_assigned.get(&v) {
                if existing != val {
                    return ProbeResult::Conflict;
                }
                continue;
            }
            sim_assigned.insert(v, val);

            let true_lit = if val { v as i32 } else { -(v as i32) };
            let false_lit = -true_lit;

            for clause in sim_clauses.iter_mut() {
                if let Some(lits) = clause {
                    if lits.contains(&true_lit) {
                        *clause = None;
                    } else {
                        lits.retain(|&l| l != false_lit);
                        if lits.is_empty() {
                            return ProbeResult::Conflict;
                        }
                        if lits.len() == 1 {
                            let unit_lit = lits[0];
                            let unit_var = unit_lit.unsigned_abs() as usize;
                            queue.push((unit_var, unit_lit > 0));
                        }
                    }
                }
            }
        }

        ProbeResult::Forced(sim_assigned)
    }

    /// Build the final PreprocessResult with compacted variable numbering.
    fn finalize(self) -> PreprocessResult {
        // Collect remaining active clauses
        let active_clauses: Vec<Vec<i32>> = self.clauses.into_iter()
            .filter_map(|c| c)
            .collect();

        // Find all variables still in the formula
        let mut active_vars: HashSet<usize> = HashSet::new();
        for clause in &active_clauses {
            for &lit in clause {
                active_vars.insert(lit.unsigned_abs() as usize);
            }
        }

        // Create compact variable mapping: old (1-based) → new (1-based)
        let mut sorted_vars: Vec<usize> = active_vars.into_iter().collect();
        sorted_vars.sort();

        let mut old_to_new: HashMap<usize, usize> = HashMap::new();
        let mut var_map: Vec<usize> = Vec::new(); // new_idx (0-based) → old_var (1-based)
        for (new_idx, &old_var) in sorted_vars.iter().enumerate() {
            old_to_new.insert(old_var, new_idx + 1);
            var_map.push(old_var);
        }

        // Remap clauses to new variable numbering
        let remapped_clauses: Vec<Vec<i32>> = active_clauses.iter()
            .map(|clause| {
                clause.iter().map(|&lit| {
                    let var = lit.unsigned_abs() as usize;
                    let new_var = old_to_new[&var] as i32;
                    if lit > 0 { new_var } else { -new_var }
                }).collect()
            })
            .collect();

        let new_num_vars = sorted_vars.len();

        // BVE-eliminated vars are reconstructed via bve_stack, not fixed_vars
        let bve_vars: HashSet<usize> = self.bve_stack.iter().map(|e| e.var).collect();
        let fixed_vars: Vec<(usize, bool)> = self.assigned.into_iter()
            .filter(|(var, _)| !bve_vars.contains(var))
            .collect();

        let mut stats = self.stats;
        stats.vars_eliminated = self.num_vars - new_num_vars;
        // clauses_eliminated already tracked incrementally

        PreprocessResult {
            clauses: remapped_clauses,
            num_vars: new_num_vars,
            var_map,
            fixed_vars,
            bve_stack: self.bve_stack,
            stats,
        }
    }
}

enum ProbeResult {
    Conflict,
    Forced(HashMap<usize, bool>),
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Helper: verify that a formula is satisfiable by the given assignment.
    // ========================================================================
    fn verify_assignment(clauses: &[Vec<i32>], assignment: &[bool]) -> bool {
        clauses.iter().all(|clause| {
            clause.iter().any(|&lit| {
                let var_idx = (lit.unsigned_abs() as usize) - 1;
                let val = assignment[var_idx];
                (lit > 0 && val) || (lit < 0 && !val)
            })
        })
    }

    /// The critical correctness test: preprocess, then verify the reconstructed
    /// assignment satisfies the ORIGINAL formula.
    fn assert_preprocess_preserves_sat(num_vars: usize, clauses: Vec<Vec<i32>>, known_solution: &[bool]) {
        assert!(
            verify_assignment(&clauses, known_solution),
            "Bug in test: known_solution doesn't satisfy the original formula"
        );

        let result = preprocess(num_vars, clauses.clone());

        if result.num_vars == 0 {
            // Fully solved by preprocessing
            let full = result.reconstruct_assignment(&[], num_vars);
            assert!(
                verify_assignment(&clauses, &full),
                "Preprocessing solved formula but reconstructed assignment is WRONG!\n\
                 Assignment: {:?}\nOriginal clauses: {:?}",
                full, clauses
            );
            return;
        }

        // Find a satisfying assignment for the reduced formula by trying
        // all 2^n combinations (only feasible for small test formulas)
        let mut found = false;
        for bits in 0..(1u64 << result.num_vars) {
            let reduced_assignment: Vec<bool> = (0..result.num_vars)
                .map(|i| (bits >> i) & 1 == 1)
                .collect();

            // Check if this assignment satisfies the reduced formula
            if verify_assignment(&result.clauses, &reduced_assignment) {
                // Reconstruct and verify against ORIGINAL
                let full = result.reconstruct_assignment(&reduced_assignment, num_vars);
                assert!(
                    verify_assignment(&clauses, &full),
                    "Reduced formula is SAT but reconstructed assignment FAILS on original!\n\
                     Reduced assignment: {:?}\nFull assignment: {:?}\n\
                     Original clauses: {:?}\nReduced clauses: {:?}\n\
                     Fixed vars: {:?}\nBVE stack len: {}",
                    reduced_assignment, full, clauses, result.clauses,
                    result.fixed_vars, result.bve_stack.len()
                );
                found = true;
                break;
            }
        }

        assert!(
            found,
            "Reduced formula is UNSAT but original was SAT! Preprocessing broke equisatisfiability.\n\
             Original ({} vars, {} clauses) → Reduced ({} vars, {} clauses)\n\
             Stats: {:?}",
            num_vars, clauses.len(), result.num_vars, result.clauses.len(), result.stats
        );
    }

    // ========================================================================
    // Simple pseudo-random number generator for deterministic test generation
    // ========================================================================
    struct TestRng(u64);
    impl TestRng {
        fn new(seed: u64) -> Self { TestRng(seed) }
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn next_range(&mut self, max: u64) -> u64 {
            self.next() % max
        }
    }

    /// Generate a random SAT formula with a planted solution.
    /// Every clause is guaranteed to be satisfied by `solution`.
    fn generate_planted_sat(
        num_vars: usize,
        num_clauses: usize,
        clause_width: usize,
        seed: u64,
    ) -> (Vec<Vec<i32>>, Vec<bool>) {
        let mut rng = TestRng::new(seed);
        let solution: Vec<bool> = (0..num_vars).map(|_| rng.next() % 2 == 0).collect();
        let mut clauses = Vec::new();

        for _ in 0..num_clauses {
            let mut clause = Vec::new();
            let mut used_vars = HashSet::new();

            while clause.len() < clause_width {
                let var = (rng.next_range(num_vars as u64) as usize) + 1;
                if used_vars.contains(&var) {
                    continue;
                }
                used_vars.insert(var);

                // At least one literal must be satisfied; for robustness
                // make each literal independently random but ensure at least one is true
                let positive = rng.next() % 2 == 0;
                let lit = if positive { var as i32 } else { -(var as i32) };
                clause.push(lit);
            }

            // Ensure clause is satisfied: if no literal matches solution, flip one
            let satisfied = clause.iter().any(|&lit| {
                let v = (lit.unsigned_abs() as usize) - 1;
                (lit > 0 && solution[v]) || (lit < 0 && !solution[v])
            });
            if !satisfied {
                // Flip the first literal to match solution
                let var = clause[0].unsigned_abs() as usize;
                clause[0] = if solution[var - 1] { var as i32 } else { -(var as i32) };
            }

            clauses.push(clause);
        }

        (clauses, solution)
    }

    // ========================================================================
    // Unit tests for individual techniques
    // ========================================================================

    #[test]
    fn test_unit_propagation_basic() {
        let clauses = vec![
            vec![1],           // unit: x1 = true
            vec![1, 2, 3],    // satisfied by x1
            vec![-1, 2],      // becomes [2] → unit: x2 = true
            vec![-2, 3],      // becomes [3] → unit: x3 = true
        ];
        let result = preprocess(3, clauses);
        assert_eq!(result.num_vars, 0);
        assert_eq!(result.clauses.len(), 0);
        assert!(result.stats.unit_props >= 1);
    }

    #[test]
    fn test_unit_propagation_correctness() {
        let clauses = vec![
            vec![1],
            vec![1, 2, 3],
            vec![-1, 2],
            vec![-2, 3],
        ];
        assert_preprocess_preserves_sat(3, clauses, &[true, true, true]);
    }

    #[test]
    fn test_pure_literal_basic() {
        let clauses = vec![
            vec![1, 2, 3],
            vec![1, -2, -3],
            vec![1, -2, 3],
        ];
        let result = preprocess(3, clauses);
        assert_eq!(result.clauses.len(), 0);
        assert!(result.stats.pure_literals >= 1);
    }

    #[test]
    fn test_pure_literal_correctness() {
        let clauses = vec![
            vec![1, 2, 3],
            vec![1, -2, -3],
            vec![1, -2, 3],
        ];
        assert_preprocess_preserves_sat(3, clauses, &[true, false, true]);
    }

    #[test]
    fn test_subsumption_fires() {
        let clauses = vec![
            vec![1, 2],
            vec![1, 2, 3],
            vec![-1, 3],
            vec![-1, -2],
            vec![1, -3],
        ];
        let result = preprocess(3, clauses);
        assert!(result.stats.subsumptions >= 1);
    }

    #[test]
    fn test_subsumption_correctness() {
        // [1,2] subsumes [1,2,3]. Need solution satisfying all 5 clauses.
        // x1=true, x2=false, x3=true: [1,2]→T, [1,2,3]→T, [-1,3]→T, [-1,-2]→T, [1,-3]→T(x1)
        let clauses = vec![
            vec![1, 2],
            vec![1, 2, 3],
            vec![-1, 3],
            vec![-1, -2],
            vec![1, -3],
        ];
        assert_preprocess_preserves_sat(3, clauses, &[true, false, true]);
    }

    #[test]
    fn test_self_subsuming_resolution_correctness() {
        let clauses = vec![
            vec![1, 2],
            vec![-1, 2, 3],
            vec![-2, -3],
            vec![1, -3],
        ];
        assert_preprocess_preserves_sat(3, clauses, &[true, true, false]);
    }

    #[test]
    fn test_bve_correctness() {
        // Variable 3 appears in exactly 2 clauses: [1, 3] and [2, -3]
        // Resolving: [1, 2]
        let clauses = vec![
            vec![1, 3],
            vec![2, -3],
        ];
        assert_preprocess_preserves_sat(3, clauses, &[true, true, true]);
    }

    #[test]
    fn test_bve_reconstruction_needs_true() {
        // After BVE resolves out var 3, the original clauses require x3=true
        // to satisfy [3, -1] when x1=true.
        let clauses = vec![
            vec![1, 3],     // sat by x1=true OR x3=true
            vec![-1, -3],   // sat by x1=false OR x3=false
            vec![1, -2],    // sat by x1=true
            vec![2, -1],    // sat by x2=true OR x1=false
        ];
        assert_preprocess_preserves_sat(3, clauses, &[true, true, false]);
    }

    #[test]
    fn test_bve_reconstruction_needs_false() {
        // BVE resolves out var 2; reconstruction must find x2 value
        let clauses = vec![
            vec![1, -2],
            vec![-1, 2],
        ];
        // Solution: x1=true, x2=true satisfies both clauses
        // (x1=true→clause1 SAT, x2=true→clause2 SAT)
        assert_preprocess_preserves_sat(2, clauses, &[true, true]);
    }

    #[test]
    fn test_failed_literal_correctness() {
        let clauses = vec![
            vec![1, 2],
            vec![-2, 3],
            vec![-1, -3],
            vec![2, 3],
        ];
        assert_preprocess_preserves_sat(3, clauses, &[false, true, true]);
    }

    // ========================================================================
    // Edge cases
    // ========================================================================

    #[test]
    fn test_empty_formula() {
        let result = preprocess(0, vec![]);
        assert_eq!(result.num_vars, 0);
        assert_eq!(result.clauses.len(), 0);
    }

    #[test]
    fn test_single_clause() {
        let clauses = vec![vec![1, -2, 3]];
        assert_preprocess_preserves_sat(3, clauses, &[true, false, true]);
    }

    #[test]
    fn test_all_unit_clauses() {
        let clauses = vec![vec![1], vec![-2], vec![3], vec![-4]];
        let result = preprocess(4, clauses.clone());
        assert_eq!(result.num_vars, 0);
        assert_preprocess_preserves_sat(4, clauses, &[true, false, true, false]);
    }

    #[test]
    fn test_duplicate_literals_in_clause() {
        // Clause with duplicate literals (malformed but shouldn't crash)
        let clauses = vec![
            vec![1, 1, 2],
            vec![-1, 3],
            vec![-2, -3],
        ];
        let result = preprocess(3, clauses);
        // Should not panic; result should be valid
        assert!(result.num_vars <= 3);
    }

    #[test]
    fn test_tautological_clause() {
        // Clause [1, -1, 2] is a tautology (always true)
        // Preprocessing should handle this without breaking
        let clauses = vec![
            vec![1, -1, 2],  // tautology
            vec![-2, 3],
            vec![2, -3],
        ];
        assert_preprocess_preserves_sat(3, clauses, &[true, true, true]);
    }

    #[test]
    fn test_preprocessing_idempotent() {
        let clauses = vec![
            vec![1, 2, 3],
            vec![-1, 2],
            vec![1, -2, 3],
        ];
        let result1 = preprocess(3, clauses.clone());
        let result2 = preprocess(result1.num_vars, result1.clauses.clone());
        assert_eq!(result1.num_vars, result2.num_vars);
        assert_eq!(result1.clauses.len(), result2.clauses.len());
    }

    #[test]
    fn test_2sat_formula() {
        // x1=true,x2=true,x3=false: [1,2]→T, [-1,3]→needs x3=T! Wrong.
        // x1=true,x2=false,x3=true: [1,2]→T(x1), [-1,3]→T(x3), [-2,-3]→T(x2=F), [1,-3]→T(x1), [2,3]→T(x3)
        let clauses = vec![
            vec![1, 2],
            vec![-1, 3],
            vec![-2, -3],
            vec![1, -3],
            vec![2, 3],
        ];
        assert_preprocess_preserves_sat(3, clauses, &[true, false, true]);
    }

    #[test]
    fn test_large_clause_width() {
        // 5-SAT clause
        let clauses = vec![
            vec![1, 2, 3, 4, 5],
            vec![-1, -2, -3, -4, -5],
            vec![1, -2, 3, -4, 5],
        ];
        assert_preprocess_preserves_sat(5, clauses, &[true, false, true, false, true]);
    }

    // ========================================================================
    // Randomized correctness tests (planted SAT instances)
    // ========================================================================

    #[test]
    fn test_planted_sat_10vars_3sat_seed1() {
        let (clauses, solution) = generate_planted_sat(10, 30, 3, 12345);
        assert_preprocess_preserves_sat(10, clauses, &solution);
    }

    #[test]
    fn test_planted_sat_10vars_3sat_seed2() {
        let (clauses, solution) = generate_planted_sat(10, 30, 3, 67890);
        assert_preprocess_preserves_sat(10, clauses, &solution);
    }

    #[test]
    fn test_planted_sat_10vars_3sat_seed3() {
        let (clauses, solution) = generate_planted_sat(10, 30, 3, 11111);
        assert_preprocess_preserves_sat(10, clauses, &solution);
    }

    #[test]
    fn test_planted_sat_20vars_3sat() {
        let (clauses, solution) = generate_planted_sat(20, 80, 3, 54321);
        assert_preprocess_preserves_sat(20, clauses, &solution);
    }

    #[test]
    fn test_planted_sat_15vars_near_threshold() {
        // α ≈ 4.27 is the hardness peak for 3-SAT
        let (clauses, solution) = generate_planted_sat(15, 64, 3, 99999);
        assert_preprocess_preserves_sat(15, clauses, &solution);
    }

    #[test]
    fn test_planted_sat_overconstrained() {
        // High clause-to-variable ratio (α ≈ 6)
        let (clauses, solution) = generate_planted_sat(10, 60, 3, 77777);
        assert_preprocess_preserves_sat(10, clauses, &solution);
    }

    #[test]
    fn test_planted_sat_underconstrained() {
        // Low clause-to-variable ratio (α ≈ 2) — lots of preprocessing expected
        let (clauses, solution) = generate_planted_sat(10, 20, 3, 33333);
        assert_preprocess_preserves_sat(10, clauses, &solution);
    }

    #[test]
    fn test_planted_sat_2sat() {
        let (clauses, solution) = generate_planted_sat(10, 30, 2, 44444);
        assert_preprocess_preserves_sat(10, clauses, &solution);
    }

    #[test]
    fn test_planted_sat_4sat() {
        let (clauses, solution) = generate_planted_sat(10, 40, 4, 55555);
        assert_preprocess_preserves_sat(10, clauses, &solution);
    }

    // ========================================================================
    // Fuzz-style: many random instances with different seeds
    // ========================================================================

    #[test]
    fn test_fuzz_preprocessing_correctness() {
        // Run 50 random planted-SAT instances through preprocessing
        // and verify every single one reconstructs correctly.
        for seed in 0..50u64 {
            let num_vars = 8 + (seed % 8) as usize;  // 8-15 vars
            let ratio = 2.0 + (seed % 5) as f64;      // ratio 2-6
            let num_clauses = (num_vars as f64 * ratio) as usize;
            let width = 2 + (seed % 3) as usize;      // 2-4 SAT

            let (clauses, solution) = generate_planted_sat(
                num_vars,
                num_clauses,
                width.min(num_vars),
                seed * 7919 + 42,
            );

            // Verify planted solution works
            assert!(
                verify_assignment(&clauses, &solution),
                "Planted solution failed for seed={}", seed
            );

            let result = preprocess(num_vars, clauses.clone());

            if result.num_vars == 0 {
                let full = result.reconstruct_assignment(&[], num_vars);
                assert!(
                    verify_assignment(&clauses, &full),
                    "Fuzz seed={}: fully solved but reconstruction WRONG", seed
                );
                continue;
            }

            // For small reduced formulas, exhaustively verify
            if result.num_vars <= 20 {
                let mut found = false;
                for bits in 0..(1u64 << result.num_vars) {
                    let reduced: Vec<bool> = (0..result.num_vars)
                        .map(|i| (bits >> i) & 1 == 1)
                        .collect();
                    if verify_assignment(&result.clauses, &reduced) {
                        let full = result.reconstruct_assignment(&reduced, num_vars);
                        assert!(
                            verify_assignment(&clauses, &full),
                            "Fuzz seed={}: reduced SAT but reconstruction FAILS original\n\
                             vars: {} → {}, clauses: {} → {}",
                            seed, num_vars, result.num_vars,
                            clauses.len(), result.clauses.len()
                        );
                        found = true;
                        break;
                    }
                }
                assert!(
                    found,
                    "Fuzz seed={}: reduced formula UNSAT but original was SAT!\n\
                     vars: {} → {}, clauses: {} → {}\nStats: {:?}",
                    seed, num_vars, result.num_vars,
                    clauses.len(), result.clauses.len(), result.stats
                );
            }
        }
    }

    // ========================================================================
    // End-to-end: preprocess + solve + reconstruct + verify
    // ========================================================================

    #[test]
    fn test_end_to_end_with_solver() {
        use crate::dmm::Params;
        use crate::formula::Formula;
        use crate::solver::{solve, SolveResult, SolverConfig, Strategy};
        use crate::integrator::Method;

        // A formula that requires actual solving (not fully reducible)
        let original_clauses = vec![
            vec![1, 2, 3],
            vec![-1, -2, -3],
            vec![1, -2, 3],
            vec![-1, 2, -3],
            vec![1, 2, -3],
            vec![-1, -2, 3],
        ];
        let num_vars = 3;

        let result = preprocess(num_vars, original_clauses.clone());

        if result.num_vars == 0 {
            let full = result.reconstruct_assignment(&[], num_vars);
            assert!(verify_assignment(&original_clauses, &full));
            return;
        }

        let mut formula = Formula::new(result.num_vars, result.clauses.clone());
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 10.0,
            strategy: Strategy::Fixed(Method::Euler),
            ..Default::default()
        };

        match solve(&mut formula, &params, &config) {
            SolveResult::Sat(reduced_assignment) => {
                // Verify reduced solution is valid for reduced formula
                let reduced_raw = formula.into_raw_clauses();
                assert!(
                    verify_assignment(&reduced_raw, &reduced_assignment),
                    "Solver returned SAT but assignment doesn't satisfy reduced formula!"
                );

                // Reconstruct and verify against original
                let full = result.reconstruct_assignment(&reduced_assignment, num_vars);
                assert!(
                    verify_assignment(&original_clauses, &full),
                    "Solver+reconstruct produced WRONG answer for original formula!\n\
                     Reduced: {:?}\nFull: {:?}",
                    reduced_assignment, full
                );
            }
            SolveResult::Unsat | SolveResult::Unknown => {
                // Timeout is acceptable for test, but the formula is trivially SAT
                // so this shouldn't happen
                panic!("Solver timed out on trivial formula");
            }
        }
    }

    #[test]
    fn test_end_to_end_planted_with_solver() {
        use crate::dmm::Params;
        use crate::formula::Formula;
        use crate::solver::{solve, SolveResult, SolverConfig, Strategy};
        use crate::integrator::Method;

        // Generate a moderate instance
        let (original_clauses, planted_solution) = generate_planted_sat(15, 50, 3, 42424);
        let num_vars = 15;

        assert!(verify_assignment(&original_clauses, &planted_solution));

        let result = preprocess(num_vars, original_clauses.clone());

        if result.num_vars == 0 {
            let full = result.reconstruct_assignment(&[], num_vars);
            assert!(verify_assignment(&original_clauses, &full));
            return;
        }

        let mut formula = Formula::new(result.num_vars, result.clauses.clone());
        let params = Params::default();
        let config = SolverConfig {
            timeout_secs: 30.0,
            strategy: Strategy::Fixed(Method::Euler),
            stagnation_check_interval: 1000,
            stagnation_patience: 5,
            max_restarts: 20,
            ..Default::default()
        };

        match solve(&mut formula, &params, &config) {
            SolveResult::Sat(reduced_assignment) => {
                let full = result.reconstruct_assignment(&reduced_assignment, num_vars);
                assert!(
                    verify_assignment(&original_clauses, &full),
                    "End-to-end: solver found SAT but reconstructed assignment is WRONG!"
                );
            }
            SolveResult::Unsat | SolveResult::Unknown => {
                // Acceptable — solver might timeout on harder instances
            }
        }
    }
}
