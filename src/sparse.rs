//! Compressed Sparse Row (CSR) matrix for sparse matrix-vector multiply.
//!
//! Hand-written, zero-dependency implementation optimized for the DMM
//! derivative computation where we need y += A * x (accumulate).

/// CSR matrix: rows are variables, columns are clauses.
pub struct CsrMatrix {
    pub num_rows: usize,
    pub num_cols: usize,
    /// row_ptr[i]..row_ptr[i+1] gives the range of nonzeros in row i.
    pub row_ptr: Vec<usize>,
    /// Column index of each nonzero.
    pub col_idx: Vec<usize>,
    /// Value of each nonzero.
    pub values: Vec<f64>,
}

impl CsrMatrix {
    /// Build CSR from unsorted triplets (row, col, val).
    /// Duplicate (row, col) entries are summed.
    pub fn from_triplets(
        num_rows: usize,
        num_cols: usize,
        triplets: &[(usize, usize, f64)],
    ) -> Self {
        // Count nonzeros per row
        let mut row_counts = vec![0usize; num_rows];
        for &(row, _, _) in triplets {
            row_counts[row] += 1;
        }

        // Build row_ptr from counts
        let mut row_ptr = vec![0usize; num_rows + 1];
        for i in 0..num_rows {
            row_ptr[i + 1] = row_ptr[i] + row_counts[i];
        }

        let nnz = row_ptr[num_rows];
        let mut col_idx = vec![0usize; nnz];
        let mut values = vec![0.0f64; nnz];

        // Fill in entries (use row_counts as cursor)
        let mut cursor = row_ptr[..num_rows].to_vec();
        for &(row, col, val) in triplets {
            let pos = cursor[row];
            col_idx[pos] = col;
            values[pos] = val;
            cursor[row] += 1;
        }

        // Sort entries within each row by column index
        for i in 0..num_rows {
            let start = row_ptr[i];
            let end = row_ptr[i + 1];
            if end - start <= 1 {
                continue;
            }
            // Simple insertion sort (rows are typically short)
            for j in (start + 1)..end {
                let key_col = col_idx[j];
                let key_val = values[j];
                let mut k = j;
                while k > start && col_idx[k - 1] > key_col {
                    col_idx[k] = col_idx[k - 1];
                    values[k] = values[k - 1];
                    k -= 1;
                }
                col_idx[k] = key_col;
                values[k] = key_val;
            }
        }

        CsrMatrix {
            num_rows,
            num_cols,
            row_ptr,
            col_idx,
            values,
        }
    }

    /// y += A * x  (accumulate into y, does NOT zero y first)
    #[inline]
    pub fn spmv_accumulate(&self, x: &[f64], y: &mut [f64]) {
        for row in 0..self.num_rows {
            let start = self.row_ptr[row];
            let end = self.row_ptr[row + 1];
            let mut sum = 0.0;
            for idx in start..end {
                sum += self.values[idx] * x[self.col_idx[idx]];
            }
            y[row] += sum;
        }
    }

    /// Number of nonzero entries.
    #[allow(dead_code)]
    pub fn nnz(&self) -> usize {
        self.row_ptr[self.num_rows]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spmv_identity() {
        // 3x3 identity matrix
        let triplets = vec![(0, 0, 1.0), (1, 1, 1.0), (2, 2, 1.0)];
        let m = CsrMatrix::from_triplets(3, 3, &triplets);
        let x = vec![3.0, 5.0, 7.0];
        let mut y = vec![0.0; 3];
        m.spmv_accumulate(&x, &mut y);
        assert_eq!(y, vec![3.0, 5.0, 7.0]);
    }

    #[test]
    fn test_spmv_accumulate() {
        let triplets = vec![(0, 0, 2.0), (0, 1, 1.0), (1, 0, 3.0)];
        let m = CsrMatrix::from_triplets(2, 2, &triplets);
        let x = vec![1.0, 2.0];
        let mut y = vec![10.0, 20.0];
        m.spmv_accumulate(&x, &mut y);
        // y[0] += 2*1 + 1*2 = 4 → 14
        // y[1] += 3*1 = 3 → 23
        assert_eq!(y, vec![14.0, 23.0]);
    }

    #[test]
    fn test_spmv_sparse_sat_pattern() {
        // Simulate MN{4} for 3 clauses, 4 vars:
        // clause 0: var 0 positive → row=0, col=0, val=1.0
        // clause 1: var 1 negative → row=1, col=1, val=-1.0
        // clause 2: var 0 positive → row=0, col=2, val=1.0
        let triplets = vec![(0, 0, 1.0), (1, 1, -1.0), (0, 2, 1.0)];
        let m = CsrMatrix::from_triplets(4, 3, &triplets);

        let rhs = vec![0.5, 0.3, 0.7]; // per-clause RHS
        let mut dv = vec![0.0; 4];
        m.spmv_accumulate(&rhs, &mut dv);
        // dv[0] = 1.0*0.5 + 1.0*0.7 = 1.2
        // dv[1] = -1.0*0.3 = -0.3
        assert!((dv[0] - 1.2).abs() < 1e-12);
        assert!((dv[1] - (-0.3)).abs() < 1e-12);
        assert_eq!(dv[2], 0.0);
        assert_eq!(dv[3], 0.0);
    }

    #[test]
    fn test_empty_matrix() {
        let m = CsrMatrix::from_triplets(3, 3, &[]);
        let x = vec![1.0, 2.0, 3.0];
        let mut y = vec![0.0; 3];
        m.spmv_accumulate(&x, &mut y);
        assert_eq!(y, vec![0.0, 0.0, 0.0]);
    }
}
