# SpinSAT Project Overview

## Purpose
SpinSAT is a Boolean satisfiability (SAT) solver based on digital memcomputing machines (DMMs). 
It maps SAT problems onto coupled ODEs where Boolean variables become continuous voltages.
The goal is to enter the International SAT Competition 2026 (Experimental Track).

## Tech Stack
- **Language**: Rust (decision made 2026-03-20)
- **Build**: Cargo
- **Competition target**: Pre-compiled static Linux binary (x86_64-unknown-linux-musl)
- **No external ODE library**: Hand-written integrators (Forward Euler, RK4, Trapezoid)
- **Generalized to k-SAT**: Not limited to 3-SAT

## Competition Details
- **Track**: Experimental (no UNSAT proof certificates required)
- **Solver type**: Incomplete (finds SAT assignments, cannot prove UNSAT)
- **Timeout**: 5000 seconds per instance
- **Environment**: Ubuntu 24.04, Intel Xeon Platinum 8368, 32 GB RAM, single-threaded
- **Submission**: GitHub repo with build.sh + run.sh
- **Registration deadline**: April 19, 2026
- **Code deadline**: April 26, 2026
- **Documentation deadline**: May 17, 2026

## Key Paper
Bearden, Pei, Di Ventra. "Efficient Solution of Boolean Satisfiability Problems with Digital MemComputing." Scientific Reports 10, 19741 (2020).
