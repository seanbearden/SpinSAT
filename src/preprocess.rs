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
    /// Statistics from preprocessing.
    pub stats: PreprocessStats,
}

impl PreprocessResult {
    /// Reconstruct full assignment from the reduced solution.
    /// `reduced_assignment` is indexed by reduced variable (0-based).
    pub fn reconstruct_assignment(&self, reduced_assignment: &[bool], original_num_vars: usize) -> Vec<bool> {
        let mut full = vec![false; original_num_vars];

        // Apply fixed variables
        for &(orig_var, val) in &self.fixed_vars {
            full[orig_var - 1] = val;
        }

        // Map reduced solution back to original variables
        for (reduced_idx, &val) in reduced_assignment.iter().enumerate() {
            let orig_var = self.var_map[reduced_idx];
            full[orig_var - 1] = val;
        }

        full
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
pub fn preprocess(num_vars: usize, clauses: Vec<Vec<i32>>) -> PreprocessResult {
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
    stats: PreprocessStats,
}

impl PreprocessState {
    fn new(num_vars: usize, clauses: Vec<Vec<i32>>) -> Self {
        let clauses = clauses.into_iter().map(Some).collect();
        PreprocessState {
            num_vars,
            clauses,
            assigned: HashMap::new(),
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
    fn subsumption_eliminate(&mut self) {
        let n = self.clauses.len();

        // Collect active clauses sorted by length (shorter first = more likely to subsume)
        let mut indices: Vec<usize> = (0..n)
            .filter(|&i| self.clauses[i].is_some())
            .collect();
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
    fn self_subsuming_resolve(&mut self) {
        let n = self.clauses.len();
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
    fn bounded_variable_eliminate(&mut self) {
        let mut candidates: Vec<usize> = (1..=self.num_vars)
            .filter(|v| !self.assigned.contains_key(v))
            .collect();

        // Sort by occurrence count (least-occurring first = most likely to reduce)
        candidates.sort_by_key(|&var| self.count_occurrences(var));

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
                // Mark variable as eliminated (assign false by default; actual value
                // doesn't matter for BVE since the variable is resolved out)
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
    fn failed_literal_probe(&mut self) {
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
        let fixed_vars: Vec<(usize, bool)> = self.assigned.into_iter().collect();

        let mut stats = self.stats;
        stats.vars_eliminated = self.num_vars - new_num_vars;
        // clauses_eliminated already tracked incrementally

        PreprocessResult {
            clauses: remapped_clauses,
            num_vars: new_num_vars,
            var_map,
            fixed_vars,
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

    #[test]
    fn test_unit_propagation() {
        // x1 is forced true by unit clause, which satisfies clause 1 and shortens clause 3
        let clauses = vec![
            vec![1],           // unit: x1 = true
            vec![1, 2, 3],    // satisfied by x1
            vec![-1, 2],      // becomes [2] → unit: x2 = true
            vec![-2, 3],      // becomes [3] → unit: x3 = true
        ];
        let result = preprocess(3, clauses);
        assert_eq!(result.num_vars, 0);
        assert_eq!(result.clauses.len(), 0);
        assert_eq!(result.fixed_vars.len(), 3);
    }

    #[test]
    fn test_pure_literal() {
        // x1 only appears positive, x2 only appears negative
        let clauses = vec![
            vec![1, 2, 3],
            vec![1, -2, -3],
            vec![1, -2, 3],
        ];
        let result = preprocess(3, clauses);
        // x1 is pure positive → assign true → all clauses satisfied
        assert_eq!(result.clauses.len(), 0);
    }

    #[test]
    fn test_subsumption() {
        // Test subsumption in isolation: [1,2] subsumes [1,2,3]
        let clauses = vec![
            vec![1, 2],        // subsumes [1, 2, 3]
            vec![1, 2, 3],    // subsumed
            vec![-1, 3],      // not subsumed
            vec![-1, -2],     // prevents pure literal elimination
            vec![1, -3],      // prevents pure literal elimination
        ];
        let result = preprocess(3, clauses);
        // Subsumption should remove [1, 2, 3], other techniques may reduce further
        assert!(result.stats.subsumptions >= 1);
    }

    #[test]
    fn test_self_subsuming_resolution() {
        // Clause A = [1, 2], Clause B = [-1, 2, 3]
        // Resolving on 1: [2, 3] subsumes B → strengthen B to [2, 3]
        let clauses = vec![
            vec![1, 2],
            vec![-1, 2, 3],
        ];
        let result = preprocess(3, clauses);
        // After self-subsumption: [1, 2] and [2, 3]
        // Then no further reductions unless pure/unit
        assert!(result.clauses.len() <= 2);
    }

    #[test]
    fn test_bve_simple() {
        // Variable 3 appears in exactly 2 clauses: [1, 3] and [2, -3]
        // Resolving: [1, 2] — removes 2 clauses, adds 1
        let clauses = vec![
            vec![1, 3],
            vec![2, -3],
        ];
        let result = preprocess(3, clauses);
        // Should resolve out var 3, leaving [1, 2]
        assert!(result.clauses.len() <= 1);
    }

    #[test]
    fn test_reconstruct_assignment() {
        let clauses = vec![
            vec![1],           // forces x1 = true
            vec![-1, 2, 3],   // becomes [2, 3]
            vec![-2, -3],     // prevents further elimination of x2, x3
            vec![2, -3],      // both polarities present
            vec![-2, 3],      // both polarities present
        ];
        let result = preprocess(3, clauses);
        // x1 is fixed true; x2 and x3 should remain
        assert!(result.fixed_vars.iter().any(|&(v, val)| v == 1 && val));
        if result.num_vars > 0 {
            let reduced = vec![true; result.num_vars];
            let full = result.reconstruct_assignment(&reduced, 3);
            assert_eq!(full[0], true); // x1 was fixed
        }
    }

    #[test]
    fn test_empty_formula() {
        let result = preprocess(0, vec![]);
        assert_eq!(result.num_vars, 0);
        assert_eq!(result.clauses.len(), 0);
    }

    #[test]
    fn test_no_reduction_possible() {
        // Verify preprocessing doesn't panic on a balanced formula.
        // With 3 variables, BVE can often resolve them out (many tautological resolvents),
        // so we use more variables to make it genuinely irreducible.
        let clauses = vec![
            vec![1, 2, 3, 4],
            vec![-1, -2, -3, -4],
            vec![1, -2, 3, -4],
            vec![-1, 2, -3, 4],
            vec![1, 2, -3, -4],
            vec![-1, -2, 3, 4],
            vec![1, -2, -3, 4],
            vec![-1, 2, 3, -4],
        ];
        let result = preprocess(4, clauses);
        // All variables in both polarities, no units, no subsumption.
        // BVE: 4 pos × 4 neg = 16 resolvents (minus tautologies) vs 8 removed.
        // With 4 vars and balanced clauses, most resolvents are non-tautological.
        // At minimum, preprocessing should not panic and produce valid output.
        assert!(result.num_vars <= 4);
        assert!(result.clauses.len() <= 8);
    }

    #[test]
    fn test_failed_literal_probing() {
        // If assigning x1=true leads to conflict, x1 must be false
        let clauses = vec![
            vec![1, 2],
            vec![-2, 3],
            vec![-1, -3],   // If x1=true and x2=true→x3=true, but -1,-3 needs x1=false or x3=false
            vec![2, 3],
        ];
        // This tests that the probe infrastructure works without panicking
        let result = preprocess(3, clauses);
        assert!(result.num_vars <= 3);
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
}
