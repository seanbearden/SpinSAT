# SpinSAT

A dynamic SAT solver based on digital memcomputing machines (DMMs).

## Introduction

SpinSAT solves Boolean satisfiability (SAT) problems by mapping them onto a system of coupled ordinary differential equations (ODEs). Instead of discrete search (DPLL/CDCL), it uses continuous-time dynamics where Boolean variables become voltages and memory variables guide the system toward satisfying assignments.

The approach is based on the research paper:
> S.R.B. Bearden, Y.R. Pei, M. Di Ventra. "Efficient Solution of Boolean Satisfiability Problems with Digital MemComputing." *Scientific Reports* 10, 19741 (2020).
> https://doi.org/10.1038/s41598-020-76666-2

Key results from the paper:
- **Power-law scaling** of integration steps for hard planted-solution 3-SAT instances (CDC class), compared to exponential scaling for WalkSAT and SID
- **No chaotic dynamics** — the system avoids exponential energy growth, unlike previous dynamical approaches
- **Collective variable updates** — long-range order enables the system to explore the solution space efficiently
- Successfully solved all tested competition instances from the 2017 and 2018 SAT Competition Random Tracks within the 5000-second timeout

## Goal

Enter SpinSAT into the [International SAT Competition 2026](https://satcompetition.github.io/2026/).

Entry requirements: https://satcompetition.github.io/2026/

## How It Works

1. **Parse** a SAT instance in DIMACS CNF format into a polarity matrix `Q`
2. **Initialize** continuous voltages `v_n ∈ [-1, 1]` and memory variables `x_{s,m}`, `x_{l,m}`
3. **Integrate** the DMM equations of motion using forward-Euler with adaptive time step
4. **Check** if all clause constraints `C_m < 1/2` — if so, threshold voltages to obtain a Boolean assignment
5. **Output** SAT + assignment, or UNKNOWN if timeout is reached

## References

- [Main paper (open access)](https://www.nature.com/articles/s41598-020-76666-2)
- [Supplementary materials](https://doi.org/10.1038/s41598-020-76666-2) (linked from main paper)
- [SAT Competition](https://satcompetition.github.io/)
