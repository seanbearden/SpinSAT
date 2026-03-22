/// Represents a SAT formula optimized for DMM integration.
///
/// Each clause stores its literals as (variable_index_0based, polarity) pairs.
/// Uses Vec<Vec<...>> — benchmarking showed this AoS layout is faster than
/// flattened SoA alternatives due to cache locality when iterating within a clause.
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

    /// Convert back to raw signed literal clauses (1-based).
    /// Used to feed into the preprocessing pipeline.
    pub fn into_raw_clauses(self) -> Vec<Vec<i32>> {
        self.clauses
            .into_iter()
            .map(|clause| {
                clause
                    .into_iter()
                    .map(|(var_idx, polarity)| {
                        let var = (var_idx + 1) as i32;
                        if polarity > 0.0 { var } else { -var }
                    })
                    .collect()
            })
            .collect()
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
    #[allow(dead_code)]
    pub fn clause_width(&self, m: usize) -> usize {
        self.clauses[m].len()
    }

    /// Add a clause to the formula (raw 1-based signed literals).
    /// Used for incorporating learned clauses from CaDiCaL.
    pub fn add_clause(&mut self, raw_clause: &[i32]) {
        let clause: Vec<(usize, f64)> = raw_clause
            .iter()
            .map(|&lit| {
                let var_idx = (lit.unsigned_abs() as usize) - 1;
                let polarity = if lit > 0 { 1.0 } else { -1.0 };
                (var_idx, polarity)
            })
            .collect();
        self.clauses.push(clause);
    }

    /// Verify a Boolean assignment satisfies the formula.
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
        let f = Formula::new(3, vec![vec![1, -2, 3], vec![-1, 2]]);
        assert_eq!(f.num_vars, 3);
        assert_eq!(f.num_clauses(), 2);
        assert_eq!(f.clause_width(0), 3);
        assert_eq!(f.clause_width(1), 2);
    }

    #[test]
    fn test_verify_sat() {
        let f = Formula::new(2, vec![vec![1, -2], vec![-1, 2]]);
        assert!(f.verify(&[true, true]));
        assert!(f.verify(&[false, false]));
        assert!(!f.verify(&[true, false]));
    }
}
