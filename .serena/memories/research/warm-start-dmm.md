# Warm-Starting DMM Solvers: Research Findings (2026-03-22)

## Key Insight
"Restarting is not restarting" — CDCL solvers preserve learnt clauses, variable activities,
and saved phases across restarts. SpinSAT currently does cold restarts (resets v, x_s, x_l;
only keeps alpha_m). This is a major untapped improvement.

**No published prior art exists on warm-starting DMM/memcomputing solvers.** This is an
open research opportunity — potential paper: "Informed Restarts for Digital Memcomputing Machines."

## What SpinSAT Throws Away (Mapped to CDCL Equivalents)
- v (voltages) → CDCL phase saving (PRESERVED in CDCL, RESET in SpinSAT)
- x_l (long-term memory) → learnt clause database (PRESERVED in CDCL, RESET in SpinSAT)
- x_s (short-term memory) → variable activity scores (PRESERVED in CDCL, RESET in SpinSAT)
- alpha_m → activity decay (PRESERVED in both — only thing SpinSAT keeps)

## Priority Techniques to Implement

### P1: Best-Assignment Voltage Saving (Phase Saving Analog) — LOW effort, HIGH impact
- Track voltage vector with lowest count_unsat across all restarts
- On restart, initialize from best-ever state + small Gaussian noise
- Kissat's "target phases" do exactly this — save best trail configuration
- Implementation: add `best_v: Vec<f64>` and `best_unsat_count: usize` to solver state

### P2: x_l Decay Transfer (Learnt Clause Analog) — LOW effort, HIGH impact
- Instead of x_l = 1.0, apply decay: x_l_new = 1.0 + decay * (x_l_old - 1.0)
- Preserves ranking of clause difficulty while reducing magnitudes
- Suggested decay_factor: 0.3-0.5 (needs tuning)
- x_l encodes "which clauses are structurally hard" — most valuable signal we produce

### P3: Restart Type Cycling (Rephasing Analog) — MEDIUM effort, HIGH impact
- Cycle through modes (inspired by Kissat rephasing):
  - Random: fresh voltages (exploration)
  - Warm: best-known voltages + noise (exploitation)
  - Anti-phase: negate best voltages (cluster hop)
  - Informed: random voltages but preserved x_l
- Anti-phase is physics-informed: at α~4.27, solutions cluster in groups separated by O(N) Hamming distance

### P4: Backbone Detection via Cross-Restart Voting — MEDIUM effort, HIGH impact
- Variables consistently near +1 or -1 across multiple restarts → likely backbone
- Freeze or strongly bias in subsequent restarts → reduces search dimensionality
- NeuroBack (ICLR 2024) showed backbone prediction helps CDCL — we can do it without ML

### P5: Time Step Reset with State Preservation (SGDR Analog) — LOW effort, MEDIUM impact
- Reset dt to maximum while keeping voltages/memory
- Large step enables escape from plateaus; preserved state means not starting from scratch

### P6: Entropy Monitoring as Convergence Diagnostic — LOW effort, MEDIUM impact
- Track distribution of |v_n(t)| — entropy stalling = solver stuck
- Could replace fixed stagnation_check_interval with information-theoretic trigger
- Needs proof it doesn't slow down the competition solver (overhead concern)

## Key CDCL References
- "Revisiting Restarts of CDCL" (arXiv:2404.16387, 2024) — warm vs cold restart study
- Biere & Fleury (2020) — rephasing in CaDiCaL/Kissat
- ReusedTrail (van der Tak et al., 2011) — avoid re-assigning identical variables
- Shaw & Meel (SAT 2020) — phase selection heuristics

## Solution Space Structure at Phase Transition
- α < ~3.86: single giant connected component (solutions navigable by small flips)
- ~3.86 < α < ~4.15: giant component shatters into exponentially many isolated clusters
- ~4.15 < α < ~4.267: weight concentrates on bounded number of dominant clusters
- α > ~4.267: UNSAT
- Anti-phase restart is principled at α~4.27 because clusters are O(N) apart

## Memcomputing-Specific
- DMMs exhibit self-averaging (Primosch et al., PRE 2023) — increasingly insensitive to ICs at larger N
- Solution equilibria have topological protection (Di Ventra & Ovchinnikov, 2019)
- Different ICs lead to different paths (possibly different solution clusters)
- No chaos, no periodic orbits — only solution equilibria (paper guarantee)
