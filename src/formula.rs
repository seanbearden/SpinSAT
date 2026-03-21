/// Represents a SAT formula in a form optimized for DMM integration.
///
/// For each clause m, we store which variables appear and their polarities.
/// The polarity q_{n,m} is +1 if variable n appears positive, -1 if negated, 0 if absent.
///
/// For efficient sparse computation, we store per-literal-position data:
/// For each literal position k in clause m:
///   - `var_idx`: which variable (0-indexed)
///   - `polarity`: +1.0 or -1.0
///   - `half_polarity`: polarity / 2.0 (pre-divided, matching MATLAB optimization)
pub struct Formula {
    pub num_vars: usize,
    /// Each clause is a list of (variable_index_0based, polarity +1/-1)
    clauses: Vec<Vec<(usize, f64)>>,
}

impl Formula {
    pub fn new(num_vars: usize, raw_clauses: Vec<Vec<i32>>) -> Self {
        let clauses = raw_clauses
            .into_iter()
            .map(|clause| {
                clause
                    .into_iter()
                    .map(|lit| {
                        let var_idx = (lit.unsigned_abs() as usize) - 1;
                        let polarity = if lit > 0 { 1.0 } else { -1.0 };
                        (var_idx, polarity)
                    })
                    .collect()
            })
            .collect();

        Formula { num_vars, clauses }
    }

    #[inline]
    pub fn num_clauses(&self) -> usize {
        self.clauses.len()
    }

    #[inline]
    pub fn clause(&self, m: usize) -> &[(usize, f64)] {
        &self.clauses[m]
    }

    #[inline]
    pub fn clause_width(&self, m: usize) -> usize {
        self.clauses[m].len()
    }

    /// Verify a Boolean assignment satisfies the formula.
    /// assignment[i] = true means variable (i+1) is TRUE.
    pub fn verify(&self, assignment: &[bool]) -> bool {
        self.clauses.iter().all(|clause| {
            clause.iter().any(|&(var_idx, polarity)| {
                let val = assignment[var_idx];
                (polarity > 0.0 && val) || (polarity < 0.0 && !val)
            })
        })
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
        assert!(f.verify(&[true, true])); // both true
        assert!(f.verify(&[false, false])); // both false
        assert!(!f.verify(&[true, false])); // second clause fails
    }
}
