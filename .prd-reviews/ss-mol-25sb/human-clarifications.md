# Human Clarifications — 2026-03-25

## Q1: Rust CLI flags
**Decision**: Add CLI flags for ALL ODE parameters (alpha initial value, alpha multipliers, alpha interval, beta, gamma, delta, epsilon). Tune all of them via Optuna.

## Q2: Phasing / MVP
**Decision**: Agreed. Phase 1 MVP = local Optuna, single machine, SQLite storage, small instance subset. Multi-VM and telemetry are later phases.

## Q3: Budget model
**Decision**: Reduced timeout (e.g., 300s) for initial tuning runs is fine. Full 5000s for final validation only. Trial-count is the primary budget control.

## Q4: Cloud SQL vs alternatives
**Decision**: Use Optuna JournalStorage on GCS bucket. Hosted in cloud, more reliable than local, avoids Cloud SQL operational overhead (IAM, VPC, monthly cost).

## Q5: Parameter search space
**Decision**: Full search space — all 6 integration strategies, all 5 restart configurations, xl_decay, restart_noise, auto_zeta, plus all ODE params.

## Q6: Multi-seed
**Decision**: 5 seeds per trial config. Use reduced timeout in initial tuning phase to keep costs manageable.

## Q7: Convergence telemetry
**Decision**: Periodic stderr snapshots parsed by Python. Store in separate trajectories.db to avoid bloating benchmarks.db. Acceptable approach.
