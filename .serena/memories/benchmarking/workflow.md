# Benchmarking Workflow (Updated 2026-03-21)

## Tools Installed
- **Kissat 4.0.4** (via homebrew) — CDCL baseline solver
- **gratchk** (built from ML source) — competition-grade SAT verifier at scripts/gratchk
- **check_sat** (custom C) — fast local verifier at scripts/check_sat (compile with `cc -O2`)
- **gtimeout** (via coreutils homebrew) — timeout command for macOS

## Running Benchmarks

### Official Recorded Runs (writes to benchmarks.db)
```bash
# Initialize DB (one-time, snapshots 31,809 instances from meta.db)
python3 scripts/init_benchmarks_db.py

# Official benchmark with full auto-detection
python3 scripts/benchmark_suite.py --suite large --record --tag v0.4.0

# Force recording even with uncommitted changes
python3 scripts/benchmark_suite.py --suite large --record --force --tag dev-test
```

### Development Runs (JSON only, no DB recording)
```bash
# Quick smoke test
python3 scripts/benchmark_suite.py --suite tiny --solver spinsat --tag mytag

# Head-to-head with Kissat
python3 scripts/benchmark_suite.py --suite medium --solver spinsat --solver kissat --timeout 120

# Custom instances
python3 scripts/benchmark_suite.py --instances path/to/*.cnf --solver spinsat --timeout 300
```

## Auto-Detection (--record flag)
The benchmark script auto-detects with zero manual input:
- **Solver version**: from `spinsat --version` (reads Cargo.toml via env!())
- **Git commit**: `git rev-parse --short HEAD` + dirty check
- **Hardware**: platform.machine() + platform.processor()
- **Rust version**: `rustc --version`
- **Parameters**: parsed from SpinSAT stderr (strategy, zeta, seed, restarts, method)

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
- uf250: SATLIB UF250 100 instances

## Results Storage
- **benchmarks.db** (SQLite): official runs with full metadata (version, commit, params, hardware)
- **results/ (JSON)**: all runs (both recorded and dev), backward compatible
- **GitHub Releases**: benchmarks.db attached to releases for researcher access

## Competition Reference Data
```bash
# Import Anniversary Track (5,355 instances x 28 solvers)
python3 scripts/import_competition_data.py \
  --anni-csv <repo>/gbd-data/anni-seq.csv \
  --anni-db <repo>/gbd-data/base.db

# Check import status
python3 scripts/import_competition_data.py --status
```

## Dashboard
- GitHub Pages: https://seanbearden.github.io/SpinSAT/dashboard/
- Datasette Lite: browser-based SQL explorer (link in README)

## gratchk Usage
```bash
./target/release/spinsat instance.cnf 2>/dev/null | grep "^v " | sed 's/^v //' > cert.lit
./scripts/gratchk sat instance.cnf cert.lit
# Should print: s VERIFIED SAT
```
