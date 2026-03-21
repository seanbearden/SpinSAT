# Suggested Commands (Updated 2026-03-21)

## Build & Test
```bash
cargo build --release
cargo test
./target/release/spinsat --version
./target/release/spinsat tests/test1.cnf
```

## Benchmarking

### Official Recorded Run
```bash
python3 scripts/init_benchmarks_db.py                    # one-time DB setup
python3 scripts/benchmark_suite.py --suite large --record --tag v0.4.0
```

### Development Run (no DB recording)
```bash
python3 scripts/benchmark_suite.py --suite tiny --solver spinsat
python3 scripts/benchmark_suite.py --suite medium --solver spinsat --solver kissat --timeout 120
```

### Compare & Analyze
```bash
python3 scripts/compare_results.py --by-size
python3 scripts/compare_results.py --progress
python3 scripts/perf_compare.py ./old_binary ./new_binary instances/*.cnf
```

## Competition Data
```bash
# Import Anniversary Track reference data
git clone https://github.com/mathefuchs/al-for-sat-solver-benchmarking-data /tmp/al-sat
python3 scripts/import_competition_data.py \
  --anni-csv /tmp/al-sat/gbd-data/anni-seq.csv \
  --anni-db /tmp/al-sat/gbd-data/base.db
python3 scripts/import_competition_data.py --status
```

## Release
```bash
# Versioning is automatic via release-plz
# Just push to main → Release PR appears → merge to release
# To manually upload benchmarks.db to a release:
gh release upload v0.4.1 benchmarks.db
```

## Verification
```bash
./target/release/spinsat instance.cnf 2>/dev/null | grep "^v " | sed 's/^v //' > cert.lit
./scripts/gratchk sat instance.cnf cert.lit
```

## DB Queries
```bash
sqlite3 benchmarks.db "SELECT * FROM runs;"
sqlite3 benchmarks.db "SELECT * FROM best_times LIMIT 10;"
sqlite3 benchmarks.db "SELECT COUNT(*) FROM competition_results;"
```
