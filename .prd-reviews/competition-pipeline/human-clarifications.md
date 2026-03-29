# Human Clarifications — Competition Pipeline PRD

Date: 2026-03-28

## Answers

1. **Budget**: $150/month acceptable (including Cloud SQL)
2. **Coverage**: All 5K anni2022 over time. Initial 60s sweep to find hard instances. Also test SAT 2025 instances.
3. **Competition rules**: Verified — Experimental Track confirmed for 2026. No UNSAT certificates needed. Deadlines: registration April 19, submission April 26, docs May 17.
4. **ML heuristics**: Do NOT defer — plan separately as own bead/effort
5. **Concurrent workloads**: Yes, benchmark + Optuna must run concurrently on shared Cloud SQL
6. **Family taxonomy**: GBD family field is canonical
7. **SAT 2025 reference data**: Search for it; Sean will also look. Per-instance data not yet publicly available as CSV — may need to run 2025 solvers ourselves against instances.

## Key Decisions Made
- Experimental Track is the target (no UNSAT proofs needed)
- $150/month hard budget cap
- 60s sweep first, then full 5000s on promising instances
- ML heuristic is separate effort, not deferred
- Must handle concurrent Optuna + benchmark workers
