/// Represents a SAT formula optimized for DMM integration.
///
/// Uses a flattened storage layout for cache-friendly access:
/// - All clause data stored contiguously in flat arrays
/// - `clause_offsets[m]` gives the start index of clause m in the flat arrays
/// - `var_indices[offset..offset+width]` gives variable indices for clause m
/// - `polarities[offset..offset+width]` gives polarities for clause m
///
/// For uniform k-SAT (all clauses same width), this gives perfect cache locality.
pub struct Formula {
    pub num_vars: usize,
    num_clauses: usize,
    /// Start offset of each clause in the flat arrays. Length = num_clauses + 1.
    clause_offsets: Vec<u32>,
    /// Flattened variable indices (0-based). Length = total literals.
    var_indices: Vec<u32>,
    /// Flattened polarities (+1.0 or -1.0). Length = total literals.
    polarities: Vec<f64>,
}

impl Formula {
    pub fn new(num_vars: usize, raw_clauses: Vec<Vec<i32>>) -> Self {
        let num_clauses = raw_clauses.len();
        let total_lits: usize = raw_clauses.iter().map(|c| c.len()).sum();

        let mut clause_offsets = Vec::with_capacity(num_clauses + 1);
        let mut var_indices = Vec::with_capacity(total_lits);
        let mut polarities = Vec::with_capacity(total_lits);

        let mut offset: u32 = 0;
        for clause in &raw_clauses {
            clause_offsets.push(offset);
            for &lit in clause {
                let var_idx = (lit.unsigned_abs() - 1) as u32;
                let polarity = if lit > 0 { 1.0 } else { -1.0 };
                var_indices.push(var_idx);
                polarities.push(polarity);
            }
            offset += clause.len() as u32;
        }
        clause_offsets.push(offset);

        Formula {
            num_vars,
            num_clauses,
            clause_offsets,
            var_indices,
            polarities,
        }
    }

    #[inline]
    pub fn num_clauses(&self) -> usize {
        self.num_clauses
    }

    /// Get the start offset and width of clause m.
    #[inline]
    pub fn clause_range(&self, m: usize) -> (usize, usize) {
        let start = self.clause_offsets[m] as usize;
        let end = self.clause_offsets[m + 1] as usize;
        (start, end - start)
    }

    /// Get variable index for literal at position `offset + i`.
    #[inline]
    pub fn var_idx(&self, pos: usize) -> usize {
        self.var_indices[pos] as usize
    }

    /// Get polarity for literal at position `offset + i`.
    #[inline]
    pub fn polarity(&self, pos: usize) -> f64 {
        self.polarities[pos]
    }

    #[inline]
    pub fn clause_width(&self, m: usize) -> usize {
        let (_, w) = self.clause_range(m);
        w
    }

    /// Verify a Boolean assignment satisfies the formula.
    pub fn verify(&self, assignment: &[bool]) -> bool {
        for m in 0..self.num_clauses {
            let (start, width) = self.clause_range(m);
            let mut sat = false;
            for i in 0..width {
                let pos = start + i;
                let var = self.var_idx(pos);
                let pol = self.polarity(pos);
                if (pol > 0.0 && assignment[var]) || (pol < 0.0 && !assignment[var]) {
                    sat = true;
                    break;
                }
            }
            if !sat {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formula_construction() {
        // (x1 ∨ ¬x2 ∨ x3) ∧ (¬x1 ∨ x2)
        let f = Formula::new(3, vec![vec![1, -2, 3], vec![-1, 2]]);
        assert_eq!(f.num_vars, 3);
        assert_eq!(f.num_clauses(), 2);
        assert_eq!(f.clause_width(0), 3);
        assert_eq!(f.clause_width(1), 2);
    }

    #[test]
    fn test_verify_sat() {
        // (x1 ∨ ¬x2) ∧ (¬x1 ∨ x2)
        let f = Formula::new(2, vec![vec![1, -2], vec![-1, 2]]);
        assert!(f.verify(&[true, true]));
        assert!(f.verify(&[false, false]));
        assert!(!f.verify(&[true, false]));
    }

    #[test]
    fn test_flattened_access() {
        let f = Formula::new(3, vec![vec![1, -2, 3], vec![-1, 2]]);
        let (start, width) = f.clause_range(0);
        assert_eq!(width, 3);
        assert_eq!(f.var_idx(start), 0); // x1
        assert_eq!(f.polarity(start), 1.0); // positive
        assert_eq!(f.var_idx(start + 1), 1); // x2
        assert_eq!(f.polarity(start + 1), -1.0); // negated
    }
}
