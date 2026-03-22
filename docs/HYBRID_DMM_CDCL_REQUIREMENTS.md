# Hybrid DMM-CDCL Cooperation — Requirements Specification

**Parent bead**: ss-iwn
**Date**: 2026-03-22
**Status**: Requirements complete, implementation phased

## Background

SpinSAT is a DMM-based SAT solver targeting the SAT Competition 2026 Experimental Track.
The DMM is an incomplete solver — it can find satisfying assignments but cannot prove UNSAT.
The Experimental Track includes both SAT and UNSAT instances (PAR-2 scoring, 5000s timeout).
Wrong answers result in full disqualification.

## Motivation

- UNSAT instances that time out cost 10,000s in PAR-2 scoring
- The DMM theory proves: no fixed points exist for UNSAT instances → dynamics evolve forever
  (Proposition VI.5, Supplementary Material)
- No periodic orbits (Theorems IX.3, IX.4) and no chaos (Corollary IX.4.1)
- A CDCL solver (CaDiCaL) can definitively determine SAT/UNSAT and generate DRAT proofs

## Design: Bidirectional Deep Cooperation

Based on Cai et al. "Deep Cooperation of CDCL and Local Search for SAT" (IJCAI 2022).
Extended for DMM ↔ CDCL bidirectional cooperation with pause/resume.

### DMM → CaDiCaL Data Transfer

| DMM State | CaDiCaL Target | Mechanism |
|-----------|---------------|-----------|
| Best voltage assignment (thresholded) | Initial phase polarities | `cadical.phase(lit)` for each variable |
| x_{l,m} long-term memory (clause difficulty) | VSIDS activity boost | `ls_conflict_num(x) = f(x_l)` per Deep Cooperation |
| Per-clause α_m values | Clause priority hints | Custom CaDiCaL modification (future) |

### CaDiCaL → DMM Data Transfer (on resume)

| CaDiCaL State | DMM Target | Mechanism |
|---------------|-----------|-----------|
| Learned clauses | Add to DMM's clause set | Extend polarity matrix Q, re-derive G and R |
| Saved phase polarities | Initial voltages | v_n = saved_phase (±1 or scaled) |
| Variable activity scores | Initial x_{l,m} values | Map VSIDS scores to long-term memory |

### Switching Protocol

**Signal-triggered** (not fixed timeout):

1. DMM runs, monitoring UNSAT indicator signals
2. Signal fires → DMM pauses (saves full state: {v, x_s, x_l, α_m, t})
3. Extract DMM→CaDiCaL data, launch CaDiCaL
4. CaDiCaL runs for a budget window
5. If CaDiCaL returns SAT/UNSAT → done
6. If CaDiCaL exhausts budget → extract CaDiCaL→DMM data
7. DMM resumes with smart restart: new initial conditions from CaDiCaL + added learned clauses
8. Repeat from step 1

### UNSAT Signal Candidates (require empirical validation)

1. **x_l reset saturation**: >X% of clauses have had x_{l,m} reset (hit max, reset to 1)
2. **C(v) stagnation**: min(C(v)) over last K steps hasn't improved
3. **α_m divergence**: per-clause α_m values growing unboundedly
4. **Best assignment stability**: best-seen assignment (min unsatisfied clauses) unchanged for K time units

**Empirical work needed**: Run DMM on known-UNSAT instances to characterize these signals.

## Implementation Phases

### Phase 1: CaDiCaL Integration (ss-93x)
- Add CaDiCaL as static library dependency
- Expose IPASIR C API: add_clause, phase, solve, val
- Verify independent solve capability
- **No dependencies**

### Phase 2: One-way DMM→CaDiCaL (ss-90q)
- Extract best assignment from DMM voltages
- Set CaDiCaL phase(lit) per variable
- Transfer clause difficulty via VSIDS boost
- Simple timeout-based trigger
- **Depends on**: Phase 1

### Phase 3: UNSAT Signal Detection (ss-80p)
- Run DMM on known-UNSAT instances (empirical study)
- Characterize dynamics: x_l growth, C(v) floor, reset patterns
- Build signal detection module
- Implement signal-triggered CaDiCaL switch
- **Depends on**: Phase 1

### Phase 4: Bidirectional Cooperation (ss-qoi)
- CaDiCaL→DMM learned clause transfer
- Phase/activity score feedback to DMM initial conditions
- Pause/resume with smart restart + new clauses
- Multi-switch during single solve
- **Depends on**: Phase 2, Phase 3

### Phase 5: DRAT Proof Logging (ss-2r3)
- Enable CaDiCaL DRAT output in UNSAT mode
- Write proof to proof.out
- Verify with drat-trim
- Enables future Main Track entry
- **Depends on**: Phase 4

## Key References

- Cai et al., "Deep Cooperation of CDCL and Local Search for SAT" (IJCAI 2022)
  - Relaxed CDCL, LS Rephasing, Conflict Frequency techniques
  - Won SAT Competition 2020 Main Track SAT category
  - GitHub: https://github.com/shaowei-cai-group/relaxed-sat
- Bearden, Pei, Di Ventra, "Efficient Solution of Boolean Satisfiability Problems with DMM"
  (Scientific Reports, 2020)
- CaDiCaL SAT Solver: https://github.com/arminbiere/cadical
  - `phase(int lit)` API for setting initial polarities
  - DRAT proof generation support
- DRAT-trim proof checker: https://github.com/marijnheule/drat-trim

## Competition Context

- **Target**: SAT Competition 2026 Experimental Track
- **Solver deadline**: April 26, 2026
- **Hardware**: Intel Xeon Platinum 8368, 5000s timeout, 32GB RAM
- **Scoring**: PAR-2 (unsolved = 10,000s penalty)
- **Wrong answer**: Full disqualification
- **Experimental Track**: Must outperform top 3 Main Track solvers on new benchmarks
- **UNSAT proofs**: Not required for Experimental Track
