# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0](https://github.com/seanbearden/SpinSAT/compare/v0.4.2...v0.5.0) - 2026-03-22

### Added

- add compile-time solution path tracing for dynamics investigation
- DRAT proof logging for UNSAT certificates
- bidirectional DMM-CaDiCaL cooperation with learned clause feedback
- add UNSAT signal detection module with CaDiCaL handoff
- add CaDiCaL CDCL fallback for UNSAT detection
- integrate CaDiCaL CDCL solver via cadical-sys

### Fixed

- use flock to prevent merge race in cloud_worker.sh
- add #[non_exhaustive] to SolveResult enum
- prevent cloud benchmark result loss on SSH disconnect

### Other

- Add research memories: warm-start DMM, diffusion analogy, paper opportunities
- Add --no-restart flag for continuous integration dynamics study
- Fix preprocessing gate: use clause count instead of clause×literal product
- add coverage for bidirectional DMM-CDCL cooperation
- Guard preprocessing against O(n²) blowup on large instances
- Fix index-out-of-bounds crash on clauses wider than 16 literals
- Fix DB persistence across releases
- Add requirements spec for hybrid DMM-CDCL cooperation
- Add 2017/2018 Random Track benchmarks for paper verification
- add conventional commits requirement to CLAUDE.md
- Update CLAUDE.md with preprocessing docs and shared benchmark paths
- Fix BVE variable reconstruction and add comprehensive preprocessing tests
- Implement CNF preprocessing pipeline with 6 techniques

## [0.4.2](https://github.com/seanbearden/SpinSAT/compare/v0.4.1...v0.4.2) - 2026-03-22

### Other

- Fix version detection for cloud benchmarks (Cargo.toml fallback)
- Add --record support to cloud benchmark mode
- Fix SSH timeout on first connect and turbo boost disable on GCP VMs
- Add GCP cloud benchmark pipeline for competition-faithful testing
- Add competition-first benchmarking policy to CLAUDE.md
- Add FORCE_JAVASCRIPT_ACTIONS_TO_NODE24 to all workflows
- Fix Node.js 20 deprecation warning in deploy-pages workflow
- Update CLAUDE.md with benchmark and dashboard management docs
- Fix dashboard URL in badge and README, re-record benchmarks as v0.4.1
- Fix dashboard JS syntax error — duplicate query statement
- Deploy dashboard via GitHub Actions (remove DB from git history)
- Revert dashboard to full benchmarks.db with instance metadata
- Add benchmark results badge linking to dashboard
- Slim dashboard DB: remove instance metadata, fix queries (56MB → 21MB)
- Import Anniversary Track competition data (5,355 instances x 28 solvers)
- Add competition instance downloader and first competition benchmarks
- Fix SQL reserved keyword error in dashboard
- Update docs: dashboard serves DB from Pages, not Releases
- Serve benchmarks.db from GitHub Pages directly (fix CORS)
- Add hybrid strategy benchmark results — no differentiation found
- Fix dashboard: skip HEAD check, fetch DB directly
- Fix dashboard DB loading — use direct download URL instead of API
- Merge binary build into release-plz workflow
- Add release-plz.toml configuration

## [0.4.1](https://github.com/seanbearden/SpinSAT/compare/v0.4.0...v0.4.1) - 2026-03-21

### Other

- Update Serena memories with versioning and benchmarking infrastructure
- Update README and CLAUDE.md with versioning, benchmarking, and dashboard docs
- Add path filters to CI workflow to prevent spurious codecov gaps
- Add GitHub Pages benchmark dashboard and update README
- Add initial CHANGELOG.md for release-plz
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
