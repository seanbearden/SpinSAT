//! Sparse matrix-based derivative computation for DMM.
//!
//! Challenger implementation that mirrors the MATLAB sparse matvec approach.
//! Groups clauses by width, builds CSR matrices per literal position, and
//! computes derivatives via vectorized array ops + sparse matrix-vector products.
//!
//! For uniform 3-SAT: single width group, 3 position matrices, branchless
//! min-finding on contiguous arrays — enables SIMD auto-vectorization.

use crate::dmm::{Derivatives, DmmState, Params};
use crate::formula::Formula;
use crate::sparse::CsrMatrix;

/// A group of clauses with the same width (number of literals).
struct WidthGroup {
    k: usize,
    /// Maps group-local index → original clause index in the formula.
    clause_map: Vec<usize>,
    /// Per-position variable indices: mi[pos][group_clause] = var_idx.
    mi: Vec<Vec<usize>>,
    /// Per-position polarity/2: mn[pos][group_clause] = polarity * 0.5.
    mn: Vec<Vec<f64>>,
    /// Per-position |polarity|/2: mp[pos][group_clause] = 0.5.
    mp: Vec<Vec<f64>>,
    /// Per-position CSR matrices (num_vars × group_size).
    /// matrices[pos] maps: for each variable, which clauses have it at position pos.
    matrices: Vec<CsrMatrix>,
    /// Fused CSR matrix (num_vars × k*group_size) for rigidity term.
    /// Concatenates all k position matrices horizontally.
    #[allow(dead_code)]
    fused_matrix: CsrMatrix,
}

/// Scratch buffers allocated once, reused every compute call.
struct Scratch {
    /// L values per position: l_vals[pos] has length = max group size.
    l_vals: Vec<Vec<f64>>,
    /// c_others[pos] = min of L values excluding position pos.
    c_others: Vec<Vec<f64>>,
    /// cmp[pos] = 1.0 if position pos is the minimum, else 0.0.
    cmp: Vec<Vec<f64>>,
    /// Per-clause fs = x_l * x_s.
    fs: Vec<f64>,
    /// Per-clause constraint value.
    c_m_local: Vec<f64>,
    /// RHS for gradient SpMV: rhs[pos] = c_others[pos] * fs.
    rhs_grad: Vec<Vec<f64>>,
    /// RHS for fused rigidity SpMV (length k * group_size).
    #[allow(dead_code)]
    rhs_rigid: Vec<f64>,
}

/// Sparse matrix derivative engine — challenger to the clause-by-clause loop.
pub struct SparseDerivEngine {
    groups: Vec<WidthGroup>,
    scratch: Scratch,
    #[allow(dead_code)]
    num_vars: usize,
}

impl SparseDerivEngine {
    /// Build from a formula. Groups clauses by width, constructs CSR matrices.
    pub fn from_formula(formula: &Formula) -> Self {
        let num_vars = formula.num_vars;
        let num_clauses = formula.num_clauses();

        // Group clause indices by width
        let mut width_map: std::collections::BTreeMap<usize, Vec<usize>> =
            std::collections::BTreeMap::new();
        for m in 0..num_clauses {
            let k = formula.clause(m).len();
            width_map.entry(k).or_default().push(m);
        }

        let mut max_group_size = 0;
        let mut max_k = 0;

        let groups: Vec<WidthGroup> = width_map
            .into_iter()
            .map(|(k, clause_indices)| {
                let group_size = clause_indices.len();
                if group_size > max_group_size {
                    max_group_size = group_size;
                }
                if k > max_k {
                    max_k = k;
                }

                // Build per-position arrays
                let mut mi = vec![vec![0usize; group_size]; k];
                let mut mn = vec![vec![0.0f64; group_size]; k];
                let mut mp = vec![vec![0.0f64; group_size]; k];

                for (gi, &orig_idx) in clause_indices.iter().enumerate() {
                    let clause = formula.clause(orig_idx);
                    for (pos, &(var_idx, polarity)) in clause.iter().enumerate() {
                        mi[pos][gi] = var_idx;
                        mn[pos][gi] = polarity * 0.5;
                        mp[pos][gi] = 0.5; // |polarity| * 0.5 = always 0.5
                    }
                }

                // Build per-position CSR matrices (num_vars × group_size)
                let matrices: Vec<CsrMatrix> = (0..k)
                    .map(|pos| {
                        let triplets: Vec<(usize, usize, f64)> = (0..group_size)
                            .map(|gi| (mi[pos][gi], gi, mn[pos][gi] * 2.0)) // polarity (±1)
                            .collect();
                        CsrMatrix::from_triplets(num_vars, group_size, &triplets)
                    })
                    .collect();

                // Build fused rigidity matrix (num_vars × k*group_size)
                // Column layout: [pos0_clause0..pos0_clauseN, pos1_clause0..pos1_clauseN, ...]
                let mut fused_triplets = Vec::with_capacity(k * group_size);
                for pos in 0..k {
                    let col_offset = pos * group_size;
                    for gi in 0..group_size {
                        fused_triplets.push((
                            mi[pos][gi],
                            col_offset + gi,
                            mn[pos][gi] * 2.0, // polarity
                        ));
                    }
                }
                let fused_matrix =
                    CsrMatrix::from_triplets(num_vars, k * group_size, &fused_triplets);

                WidthGroup {
                    k,
                    clause_map: clause_indices,
                    mi,
                    mn,
                    mp,
                    matrices,
                    fused_matrix,
                }
            })
            .collect();

        // Allocate scratch buffers for the largest group
        let scratch = Scratch {
            l_vals: vec![vec![0.0; max_group_size]; max_k],
            c_others: vec![vec![0.0; max_group_size]; max_k],
            cmp: vec![vec![0.0; max_group_size]; max_k],
            fs: vec![0.0; max_group_size],
            c_m_local: vec![0.0; max_group_size],
            rhs_grad: vec![vec![0.0; max_group_size]; max_k],
            rhs_rigid: vec![0.0; max_k * max_group_size],
        };

        SparseDerivEngine {
            groups,
            scratch,
            num_vars,
        }
    }

    /// Compute all derivatives — drop-in replacement for compute_derivatives.
    pub fn compute(
        &mut self,
        _formula: &Formula,
        state: &DmmState,
        params: &Params,
        derivs: &mut Derivatives,
    ) {
        // Zero voltage derivatives
        for d in derivs.dv.iter_mut() {
            *d = 0.0;
        }

        for gi in 0..self.groups.len() {
            let k = self.groups[gi].k;
            let gs = self.groups[gi].clause_map.len();

            // Phase 1: Gather L values
            for pos in 0..k {
                for ci in 0..gs {
                    let mi_val = self.groups[gi].mi[pos][ci];
                    let mn_val = self.groups[gi].mn[pos][ci];
                    let mp_val = self.groups[gi].mp[pos][ci];
                    self.scratch.l_vals[pos][ci] = mp_val - mn_val * state.v[mi_val];
                }
            }

            // Phase 2: Find min, c_others, cmp per clause
            compute_min(k, gs, &mut self.scratch);

            // Phase 3: Memory derivatives + c_m writeback
            for ci in 0..gs {
                let orig = self.groups[gi].clause_map[ci];
                let cm = self.scratch.c_m_local[ci]; // L values already include 0.5 factor
                derivs.c_m[orig] = cm;
                derivs.dx_s[orig] =
                    params.beta * (state.x_s[orig] + params.epsilon) * (cm - params.gamma);
                derivs.dx_l[orig] = state.alpha_m[orig] * (cm - params.delta);
                self.scratch.fs[ci] = state.x_l[orig] * state.x_s[orig];
            }

            // Phase 4: Gradient SpMV
            for pos in 0..k {
                for ci in 0..gs {
                    self.scratch.rhs_grad[pos][ci] =
                        self.scratch.c_others[pos][ci] * self.scratch.fs[ci];
                }
                self.groups[gi].matrices[pos]
                    .spmv_accumulate(&self.scratch.rhs_grad[pos][..gs], &mut derivs.dv);
            }

            // Phase 5: Rigidity — direct scatter (only min literal contributes)
            // Simpler and more efficient than SpMV since exactly 1 literal
            // per clause contributes to rigidity.
            for pos in 0..k {
                for ci in 0..gs {
                    if self.scratch.cmp[pos][ci] > 0.5 {
                        let orig = self.groups[gi].clause_map[ci];
                        let var_idx = self.groups[gi].mi[pos][ci];
                        let polarity = self.groups[gi].mn[pos][ci] * 2.0;
                        let r_nm = 0.5 * (polarity - state.v[var_idx]);
                        let cm = derivs.c_m[orig];
                        let xl = state.x_l[orig];
                        let xs = state.x_s[orig];
                        derivs.dv[var_idx] +=
                            (1.0 + params.zeta * xl) * cm * (1.0 - xs) * r_nm;
                    }
                }
            }
        }
    }
}

/// Compute min, c_others, and cmp arrays from l_vals.
/// Dispatches to specialized k=2, k=3, or generic path.
fn compute_min(k: usize, gs: usize, scratch: &mut Scratch) {
    match k {
        1 => {
            // Unit clauses: min is the only literal
            for ci in 0..gs {
                scratch.c_m_local[ci] = scratch.l_vals[0][ci];
                scratch.cmp[0][ci] = 1.0;
                scratch.c_others[0][ci] = f64::MAX; // no other literals
            }
        }
        2 => {
            for ci in 0..gs {
                let a = scratch.l_vals[0][ci];
                let b = scratch.l_vals[1][ci];
                if a <= b {
                    scratch.c_m_local[ci] = a;
                    scratch.cmp[0][ci] = 1.0;
                    scratch.cmp[1][ci] = 0.0;
                } else {
                    scratch.c_m_local[ci] = b;
                    scratch.cmp[0][ci] = 0.0;
                    scratch.cmp[1][ci] = 1.0;
                }
                scratch.c_others[0][ci] = b;
                scratch.c_others[1][ci] = a;
            }
        }
        3 => {
            for ci in 0..gs {
                let a = scratch.l_vals[0][ci];
                let b = scratch.l_vals[1][ci];
                let c = scratch.l_vals[2][ci];

                scratch.c_others[0][ci] = b.min(c);
                scratch.c_others[1][ci] = a.min(c);
                scratch.c_others[2][ci] = a.min(b);

                let ab = if a <= b { 1.0 } else { 0.0 };
                let ac = if a <= c { 1.0 } else { 0.0 };
                let bc = if b <= c { 1.0 } else { 0.0 };

                scratch.cmp[0][ci] = ab * ac;
                scratch.cmp[1][ci] = (1.0 - ab) * bc;
                scratch.cmp[2][ci] = (1.0 - ac) * (1.0 - bc);

                scratch.c_m_local[ci] =
                    scratch.cmp[0][ci] * a + scratch.cmp[1][ci] * b + scratch.cmp[2][ci] * c;
            }
        }
        _ => {
            // Generic: prefix/suffix min for arbitrary k
            let mut prefix = vec![f64::MAX; k];
            let mut suffix = vec![f64::MAX; k];

            for ci in 0..gs {
                prefix[0] = f64::MAX;
                for pos in 1..k {
                    prefix[pos] = prefix[pos - 1].min(scratch.l_vals[pos - 1][ci]);
                }
                suffix[k - 1] = f64::MAX;
                for pos in (0..k - 1).rev() {
                    suffix[pos] = suffix[pos + 1].min(scratch.l_vals[pos + 1][ci]);
                }

                let mut min_val = f64::MAX;
                let mut min_pos = 0;
                for pos in 0..k {
                    let l_val = scratch.l_vals[pos][ci];
                    scratch.c_others[pos][ci] = prefix[pos].min(suffix[pos]);
                    scratch.cmp[pos][ci] = 0.0;
                    if l_val < min_val {
                        min_val = l_val;
                        min_pos = pos;
                    }
                }
                scratch.c_m_local[ci] = min_val;
                scratch.cmp[min_pos][ci] = 1.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dmm::{compute_derivatives, DmmState, Params};
    use crate::formula::Formula;

    /// Compare sparse engine output against the loop-based champion.
    fn assert_derivatives_match(formula: &Formula, seed: u64) {
        let params = Params::default();
        let mut state = DmmState::new(formula, seed, &params);
        state.init_short_memory(formula);

        // Champion: loop-based
        let mut derivs_loop = Derivatives::new(formula.num_vars, formula.num_clauses());
        compute_derivatives(formula, &state, &params, &mut derivs_loop);

        // Challenger: sparse
        let mut engine = SparseDerivEngine::from_formula(formula);
        let mut derivs_sparse = Derivatives::new(formula.num_vars, formula.num_clauses());
        engine.compute(formula, &state, &params, &mut derivs_sparse);

        let eps = 1e-10;

        // Compare c_m
        for m in 0..formula.num_clauses() {
            assert!(
                (derivs_loop.c_m[m] - derivs_sparse.c_m[m]).abs() < eps,
                "c_m[{}] mismatch: loop={}, sparse={}",
                m,
                derivs_loop.c_m[m],
                derivs_sparse.c_m[m]
            );
        }

        // Compare dx_s
        for m in 0..formula.num_clauses() {
            assert!(
                (derivs_loop.dx_s[m] - derivs_sparse.dx_s[m]).abs() < eps,
                "dx_s[{}] mismatch: loop={}, sparse={}",
                m,
                derivs_loop.dx_s[m],
                derivs_sparse.dx_s[m]
            );
        }

        // Compare dx_l
        for m in 0..formula.num_clauses() {
            assert!(
                (derivs_loop.dx_l[m] - derivs_sparse.dx_l[m]).abs() < eps,
                "dx_l[{}] mismatch: loop={}, sparse={}",
                m,
                derivs_loop.dx_l[m],
                derivs_sparse.dx_l[m]
            );
        }

        // Compare dv
        for n in 0..formula.num_vars {
            assert!(
                (derivs_loop.dv[n] - derivs_sparse.dv[n]).abs() < eps,
                "dv[{}] mismatch: loop={}, sparse={}",
                n,
                derivs_loop.dv[n],
                derivs_sparse.dv[n]
            );
        }
    }

    #[test]
    fn test_sparse_matches_loop_3sat_tiny() {
        let f = Formula::new(3, vec![vec![1, -2, 3], vec![-1, 2, -3]]);
        assert_derivatives_match(&f, 42);
    }

    #[test]
    fn test_sparse_matches_loop_3sat_small() {
        // 10 vars, ~43 clauses (ratio 4.3)
        let mut clauses = Vec::new();
        let mut seed: u64 = 12345;
        for _ in 0..43 {
            let mut clause = Vec::new();
            for _ in 0..3 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let var = (seed % 10) as i32 + 1;
                let sign = if (seed >> 32) & 1 == 0 { 1 } else { -1 };
                clause.push(sign * var);
            }
            clauses.push(clause);
        }
        let f = Formula::new(10, clauses);
        assert_derivatives_match(&f, 42);
        assert_derivatives_match(&f, 99);
    }

    #[test]
    fn test_sparse_matches_loop_2sat() {
        let f = Formula::new(
            4,
            vec![
                vec![1, -2],
                vec![-1, 3],
                vec![2, -4],
                vec![-3, 4],
                vec![1, 4],
            ],
        );
        assert_derivatives_match(&f, 42);
    }

    #[test]
    fn test_sparse_matches_loop_mixed_k() {
        // Mix of 2-literal and 3-literal clauses
        let f = Formula::new(
            5,
            vec![
                vec![1, -2],       // k=2
                vec![-1, 3, 4],    // k=3
                vec![2, -5],       // k=2
                vec![-3, 4, -5],   // k=3
                vec![1, 2, -3],    // k=3
            ],
        );
        assert_derivatives_match(&f, 42);
    }

    #[test]
    fn test_sparse_matches_loop_k4() {
        let f = Formula::new(
            5,
            vec![
                vec![1, -2, 3, -4],
                vec![-1, 2, -3, 5],
                vec![4, -5, 1, -2],
            ],
        );
        assert_derivatives_match(&f, 42);
    }

    #[test]
    fn test_sparse_matches_loop_single_clause() {
        let f = Formula::new(3, vec![vec![1, -2, 3]]);
        assert_derivatives_match(&f, 42);
    }

    #[test]
    fn test_sparse_matches_loop_unit_clauses() {
        // Unit clauses (k=1) — edge case
        let f = Formula::new(3, vec![vec![1], vec![-2], vec![3]]);
        assert_derivatives_match(&f, 42);
    }

    #[test]
    fn test_sparse_matches_loop_large_uniform_3sat() {
        // 100 vars, 430 clauses — closer to competition scale
        let mut clauses = Vec::new();
        let mut seed: u64 = 999;
        for _ in 0..430 {
            let mut clause = Vec::new();
            for _ in 0..3 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let var = (seed % 100) as i32 + 1;
                let sign = if (seed >> 32) & 1 == 0 { 1 } else { -1 };
                clause.push(sign * var);
            }
            clauses.push(clause);
        }
        let f = Formula::new(100, clauses);
        assert_derivatives_match(&f, 42);
    }

    #[test]
    fn test_sparse_multiple_steps() {
        // Run both engines for multiple steps and verify they stay in sync
        let f = Formula::new(5, vec![vec![1, -2, 3], vec![-1, 4, -5], vec![2, -3, 5]]);
        let params = Params::default();
        let mut state_loop = DmmState::new(&f, 42, &params);
        state_loop.init_short_memory(&f);
        let mut state_sparse = state_loop.clone();

        let mut derivs_loop = Derivatives::new(f.num_vars, f.num_clauses());
        let mut derivs_sparse = Derivatives::new(f.num_vars, f.num_clauses());
        let mut engine = SparseDerivEngine::from_formula(&f);

        for _ in 0..10 {
            compute_derivatives(&f, &state_loop, &params, &mut derivs_loop);
            engine.compute(&f, &state_sparse, &params, &mut derivs_sparse);

            // Verify match
            for n in 0..f.num_vars {
                assert!(
                    (derivs_loop.dv[n] - derivs_sparse.dv[n]).abs() < 1e-10,
                    "dv diverged"
                );
            }

            // Euler step both (same dt)
            let dt = 0.01;
            for i in 0..f.num_vars {
                state_loop.v[i] =
                    (state_loop.v[i] + dt * derivs_loop.dv[i]).clamp(-1.0, 1.0);
                state_sparse.v[i] =
                    (state_sparse.v[i] + dt * derivs_sparse.dv[i]).clamp(-1.0, 1.0);
            }
            for i in 0..f.num_clauses() {
                state_loop.x_s[i] =
                    (state_loop.x_s[i] + dt * derivs_loop.dx_s[i]).clamp(0.0, 1.0);
                state_sparse.x_s[i] =
                    (state_sparse.x_s[i] + dt * derivs_sparse.dx_s[i]).clamp(0.0, 1.0);
                state_loop.x_l[i] = (state_loop.x_l[i] + dt * derivs_loop.dx_l[i])
                    .clamp(1.0, state_loop.max_xl);
                state_sparse.x_l[i] = (state_sparse.x_l[i] + dt * derivs_sparse.dx_l[i])
                    .clamp(1.0, state_sparse.max_xl);
            }
        }
    }
}
