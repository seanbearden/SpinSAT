# qhid ("q-hidden") SAT Instances — Deceptive Planted Solutions

## Key Paper
Jia, Moore, Strain — "Generating Hard Satisfiable Formulas by Hiding Solutions Deceptively"
- AAAI 2005, JAIR 2007 (Vol 28, pp 107-118)
- arXiv: cs/0503044

## How qhid Works
Clauses selected with probability proportional to q^t where t = literals satisfied by planted solution.
- q = 1 (1-hidden): trivially easy, majority heuristic finds solution
- q = q* ≈ 0.618 (golden ratio for k=3): BALANCED — zero signal, no bias toward or away from solution
- q < q*: DECEPTIVE — formula actively points AWAY from the planted solution

At α_r = 5.5 (competition instances), deeply overconstrained. No alternate solutions exist —
only the planted solution A, but the clause structure pushes solvers the wrong way.

## Why qhid Is Hard for SpinSAT's DMM Dynamics
- Gradient G_{n,m} and rigidity R_{n,m} are computed from clause constraints
- Deceptive clause distribution makes these gradients collectively push voltages AWAY from solution
- Long-term memory x_l can eventually overcome this, but requires crossing exponential energy barrier
- Unlike barthel (near threshold α_r≈4.3), gradient carries NO genuine signal about solution direction

## Comparison to Other Planted Instance Families
| Family | α_r | Mechanism | DMM difficulty |
|--------|-----|-----------|---------------|
| barthel (CDC) | 4.3 | Phase transition, glassy but honest gradient | Easy (median <10s) |
| komb | 5.205 | Unknown generator, moderate overconstraining | Easy (median ~6s) |
| qhid | 5.5 | Deceptive gradient, exponential barrier | HARD (some timeout at 5000s) |

## Generator
concealSATgen: https://github.com/FlorianWoerz/concealSATgen
Implements both CDC (barthel) and q-hidden algorithms. Use -p to set q parameter.

## SAT Competition History
- qhid appeared in Random Track 2017 and 2018 (as fla-qhid-*)
- Random Track discontinued after 2018 — no newer variants in competitions

## Related Theory
- Feldman, Perkins, Vempala (2015): distribution complexity hierarchy, q-hidden is high-quietness
- Krzakala & Zdeborova (2009): "quiet" planted distributions statistically indistinguishable from random

## Investigation Strategy (2026-03-22)
The exponential energy barrier is the core problem. Strategy: AVOID the barrier, don't fight it.
- Use trace tool to identify WHEN the solver encounters the barrier (signature: persistent
  convergence to non-solutions, high x_l across all clauses, voltage oscillation without progress)
- "Tunnel" through the barrier using targeted restart techniques
- Detect deceptive structure from dynamics signals → trigger escape mechanism
- Key insight: if gradient is deceptive, following it harder won't help — need to break the
  deception by injecting information the formula doesn't provide
