# Paper Opportunities from DMM Research (2026-03-22)

## Paper 1: "Informed Restarts for Digital Memcomputing Machines"
- **Status**: No prior art exists — open research opportunity
- **Core contribution**: Apply CDCL warm-restart principles to DMM solvers
- **Techniques**: voltage saving, x_l decay transfer, restart cycling, backbone detection
- **Evidence needed**: A/B benchmarks on competition instances showing improvement
- **Venue**: SAT Conference, or Scientific Reports (to pair with original DMM paper)
- **Foundation work**: implement P1 (voltage saving) and P2 (x_l decay) first,
  benchmark against cold restarts on Anniversary Track instances

## Paper 2: "Digital Memcomputing as Problem-Conditioned Flow Matching"
- **Status**: Theoretical — needs formalization
- **Core contribution**: Formalize the DMM-diffusion analogy mathematically
- **Key insight**: DMM defines an analytically guaranteed velocity field;
  diffusion models learn one without guarantees
- **Angle**: Can guarantees from DMM topology (no chaos, solution-only equilibria)
  inform better diffusion model design for combinatorial optimization?
- **Venue**: ICLR, NeurIPS, or Physical Review Research
- **Prerequisite**: Paper 1 results to demonstrate practical relevance

## Paper 3: "Attention Mechanisms in Continuous SAT Solving" (Speculative)
- **Status**: Very early — needs exploration
- **Core question**: Can the Potts model / attention equivalence inform new DMM
  equation terms that allow clauses to dynamically weight variable contributions?
- **Risk**: May not improve performance, purely theoretical
- **Prerequisite**: Strong results from Papers 1 and 2

## Priority
Paper 1 first (practical, benchmarkable, no prior art).
Paper 2 builds on Paper 1 results.
Paper 3 is exploratory — note in bead, don't pursue yet.
