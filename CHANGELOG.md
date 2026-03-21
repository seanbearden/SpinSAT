# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/seanbearden/SpinSAT/releases/tag/v0.4.0) - 2026-03-21

### Other

- Add automated versioning (release-plz) and benchmarks database
- Implement hybrid restart strategy with 5 method selection modes
- Update implementation plan — all 5 phases complete
- Add Codecov component analytics for per-module coverage tracking
- Add numerical analysis test suite with theoretically-justified tolerances
- Expand test coverage from 69% to 84%
- Fix CI: upgrade checkout to v6, use token in with block
- Bump version to v0.3.1 to trigger Codecov coverage report
- Fix Codecov badge URL to match registered repo name
- Add Codecov test results via nextest JUnit XML profile
- Drop JUnit XML upload — nextest version lacks --message-format junit
- Consolidate CI to single test run with coverage + JUnit output
- Fix nextest JUnit output: use redirect instead of --output-file
- Fix codecov action input: report-type → report_type
- Fix compiler warnings: remove unused alpha field, suppress clause_width
- Add GitHub Actions CI with Codecov coverage and test results
- Move SAT competition research to docs/
- Add competition TODO checklist and development tools to README
- Implement Phase 5: competition submission packaging
- Add hybrid restart strategy design plan to Serena memories
- Add Euler vs Trapezoid benchmark findings to Serena memory
- Update docs with Phase 4 findings, add timing rules to CLAUDE.md
- Revert formula flattening after controlled A/B testing
- Complete Phase 4: flatten formula storage for cache-friendly access
- Implement Phase 2: RK4 and Trapezoid integrators with --method flag
- Fix auto-zeta interpolation and fuse derivative computation into single pass
- Implement Phase 3: restarts, per-clause alpha_m, auto-zeta, CLI flags
- Add large suite baseline results (500-2000 vars)
- Add benchmarking infrastructure with Kissat baseline comparison
- Implement Phase 1: core DMM solver in Rust
- Update docs with implementation decisions and initialize Serena project
- Add project documentation and research papers
- Initial commit
