# DMM-Diffusion Model Analogy: Research Findings (2026-03-22)

## Core Finding
DMM is structurally equivalent to a diffusion model reverse process. Both transport noise
to structure via a velocity field. The analogy is mathematical, not metaphorical.

| Diffusion Model | DMM |
|---|---|
| x_T ~ noise distribution | v(0) ~ random in [-1,1] |
| Reverse ODE: dx/dt = score(x,t) | DMM ODE: dv/dt = f(v, x_s, x_l, Q) |
| Score learned from data | Velocity field derived from problem (Q matrix) |
| Progressive denoising → clean sample | Progressive polarization → satisfying assignment |
| Noise schedule σ(t) | Adaptive time step Δt |
| t=0 is clean data | C_m < 1/2 for all m (termination) |

## DMM Advantage
Velocity field analytically guaranteed: solution-only equilibria, no chaos, no periodic orbits.
Diffusion models have no such guarantees about their learned score functions.

## DMM Disadvantage
Cannot be "conditioned" on additional information like diffusion models can.
**Question**: Can we add conditioning mechanisms to the DMM equations?

## Transformer-ODE Connection
- Transformers ARE discretized ODEs (Tong et al., ICLR 2025)
- Self-attention = generalized Potts model conditional distribution (Rende et al., PRR 2024)
- DMM x_s switching ≈ attention weights (determines how strongly clause m "attends to" its variables)
- Both exhibit phase transitions (α~4.27 in SAT ↔ positional/semantic learning in transformers)

## GNN-SDP Connection
- GNN message passing implicitly solves SDP relaxation of MAX-SAT (Hula et al., CIKM 2024)
- NeuroSAT embeddings monotonically increase SDP objective during message passing
- DMM voltages similarly minimize a Lyapunov function (total clause dissatisfaction)
- OptGNN (NeurIPS 2024 Spotlight): polynomial GNNs can represent UGC-optimal SDP algorithms

## Per-Variable Confidence (Extractable from DMM)
- |v_n(t)| → natural confidence measure (near ±1 = decided, near 0 = uncertain)
- dv_n/dt magnitude → high derivative = variable still being pulled = low confidence
- Temporal variance → oscillating = low confidence, converging = high confidence
- These can inform backbone detection and partial restart strategies

## Denoising SAT (Direct Prior Art)
- DIFUSCO (Sun & Yang, NeurIPS 2023): diffusion-based combinatorial optimization
- "Denoising Diffusion for Sampling SAT Solutions" (NeurIPS 2022 Workshop): categorical diffusion for SAT
- torchmSAT (Hosny & Reda, 2024): differentiable MaxSAT via backpropagation
- Continuous Relaxation Annealing (NeurIPS 2024): rounding-free discrete enforcement

## Exploration Opportunity: Attention-Like Mechanism in DMM
The x_s memory switching between gradient-like and rigidity dynamics is analogous to
attention weights. Could an explicit attention mechanism be added to the DMM equations
to allow clauses to "attend" to the most informative variables? This is speculative
but theoretically motivated by the Potts model equivalence.

## Key Papers
- Tong et al., "Neural ODE Transformers," ICLR 2025
- Rende et al., "Mapping attention to generalized Potts model," PRR 2024
- Hula et al., "Understanding GNNs for SAT through SDP," CIKM 2024
- Yau et al., "OptGNN," NeurIPS 2024 Spotlight
- Sun & Yang, "DIFUSCO," NeurIPS 2023
- Bearden, Pei & Di Ventra, Scientific Reports 2020
- Di Ventra & Ovchinnikov, "DMM: Logic to Dynamics to Topology," 2019
