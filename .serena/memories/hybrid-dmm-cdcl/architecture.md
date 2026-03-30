# Hybrid DMM-CaDiCaL Architecture (2026-03-29)

## Overview
SpinSAT uses a bidirectional DMM-CaDiCaL hybrid for UNSAT detection. The DMM solver handles SAT instances natively. For potential UNSAT instances, signals trigger handoff to CaDiCaL (CDCL solver via FFI).

## Key Files
- `src/cdcl.rs` — CaDiCaL FFI wrapper (CdclSolver struct, phase hints, assumptions)
- `src/unsat_signal.rs` — Detects stagnation signals that suggest UNSAT (UnsatSignalDetector)
- `src/solver.rs` — Main solve loop with adaptive strategy, signal-triggered handoffs, final CDCL fallback

## Signal Detection (`UnsatSignalDetector`)
Monitors DMM integration for stagnation patterns suggesting UNSAT:
- `SignalKind::Stagnation` — unsat count plateaus despite long-term memory growth
- `SignalKind::MemorySaturation` — x_l values hitting ceiling across many clauses
- Tracks `best_assignment` (lowest unsat count seen) for seeding CaDiCaL phases

Config: `SignalConfig` has thresholds for stagnation window, memory saturation ratio, etc.

## Handoff Flow (try_cdcl_handoff)
1. DMM detects stagnation signal
2. Extract DMM state: voltages → CaDiCaL phase hints, x_l → frustrated variable assumptions
3. CaDiCaL runs with bounded conflict budget (`cdcl_conflict_budget`, default 100K)
4. If CaDiCaL returns SAT/UNSAT → done
5. If UNKNOWN (budget exhausted) → feed CaDiCaL's fixed literals + phases back to DMM
6. DMM resumes integration with CaDiCaL-informed state

## CDCL Fallback (cdcl_fallback)
After DMM timeout, a final CaDiCaL run gets all remaining time:
- Seeded with DMM's best voltages as phase hints
- Top-k frustrated variables (highest x_l) set as assumptions
- No conflict limit — runs until timeout

## Adaptive Strategy
`Strategy::Adaptive` rotates integration methods (Euler, RK4, Trapezoid) based on effectiveness:
- Tracks unsat reduction rate per method
- Switches to best-performing method dynamically
- Confidence-based DMM/CaDiCaL time budget split

## Adaptive Budget Parameters (from tuning)
- Initial DMM share: 40% of timeout
- confidence_decay: 0.4 per stagnant restart
- confidence_boost: 0.05 on genuine improvement
- min_dmm_share: 5% of remaining time
- Mid-solve CaDiCaL: triggers at confidence < 0.7 AND consecutive_stagnant >= 1
- Top-k frustrated vars: 10% of num_vars, capped at 50

## SolverConfig Flags
- `cdcl_fallback: bool` — enable final CaDiCaL fallback after DMM timeout
- `enable_unsat_detection: bool` — enable signal-triggered mid-solve handoffs
- `cdcl_conflict_budget: i32` — conflict limit per handoff attempt (default 100K)
- `proof_path: Option<String>` — DRAT proof output path

## Competition Relevance
- Experimental Track does NOT require UNSAT proofs
- But detecting UNSAT early saves timeout penalty (PAR-2 = 2 × 5000s)
- CaDiCaL can prove UNSAT on structured instances where DMM stagnates
- DMM handles SAT instances without any overhead (adaptive budget only activates on stagnation)
