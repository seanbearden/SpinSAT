# Benchmarking Workflow

## Tools Installed
- **Kissat 4.0.4** (via homebrew) — CDCL baseline solver
- **gratchk** (built from ML source) — competition-grade SAT verifier at scripts/gratchk
- **check_sat** (custom C) — fast local verifier at scripts/check_sat (compile with `cc -O2`)
- **gtimeout** (via coreutils homebrew) — timeout command for macOS

## Running Benchmarks
```bash
# Quick smoke test
python3 scripts/benchmark_suite.py --suite tiny --solver spinsat --tag mytag

# Head-to-head with Kissat
python3 scripts/benchmark_suite.py --suite medium --solver spinsat --solver kissat --timeout 120 --tag v0.2

# Custom instances
python3 scripts/benchmark_suite.py --instances path/to/*.cnf --solver spinsat --timeout 300
```

## Comparing Results
```bash
python3 scripts/compare_results.py                  # all runs
python3 scripts/compare_results.py --latest 3        # last 3 runs
python3 scripts/compare_results.py --by-size         # breakdown by var count
python3 scripts/compare_results.py --progress        # track improvement
```

## Available Suites
- tiny: 20-50 vars (smoke test)
- small: 100-250 vars (10 each)
- medium: 250-500 vars (20+10+10)
- large: 500-2000 vars (10+5+5+3+3)
- uf250: SATLIB UF250 100 instances (need to fix format for Kissat)

## Results Storage
JSON files in results/ directory with: instance name, solver, status, time, verified, num_vars, num_clauses, ratio

## Phase 1 Baseline (planted 3-SAT, ratio 4.3)
- Small: SpinSAT PAR-2=2.37 vs Kissat PAR-2=1.30 (40/40 both)
- Medium: SpinSAT PAR-2=27.53 vs Kissat PAR-2=11.42 (40/40 both)
- SpinSAT competitive on easy instances, gap widens on harder ones
- 1000-var hard instance: SpinSAT 264s vs Kissat 7s (37x gap)
- 1500-var: 1 SpinSAT timeout at 300s, Kissat solved in 1.9s

## SATLIB UF250 Note
- Files have leading spaces on clause lines — Kissat rejects them
- SpinSAT handles them fine
- Need to strip whitespace for Kissat comparison

## gratchk Usage
```bash
# Extract assignment and verify
./target/release/spinsat instance.cnf 2>/dev/null | grep "^v " | sed 's/^v //' > cert.lit
./scripts/gratchk sat instance.cnf cert.lit
# Should print: s VERIFIED SAT
```
