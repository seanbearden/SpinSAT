# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.4](https://github.com/seanbearden/SpinSAT/compare/v0.5.3...v0.5.4) - 2026-03-28

### Added

- completion alert via Cloud Monitoring metric push
- GCP alerting, idle VM auto-stop, and completion notifications
- reusable monitoring.py module with custom Cloud Monitoring metrics
- GCS results durability + VM labels for cloud benchmarks
- distributed Optuna tuning on GCP spot VMs with PostgreSQL storage

### Fixed

- cloud Optuna robustness — retry loop, connection limits, idempotent DB setup
- remove extra labels from metric push (hyphen chars invalid)
- correct monitoring.py imports and VM scope for Cloud Monitoring

## [0.5.3](https://github.com/seanbearden/SpinSAT/compare/v0.5.2...v0.5.3) - 2026-03-26

### Added

- core Optuna tuning loop with TPE, pruning, multi-seed PAR-2 (ss-3qr)
- add run_solver_with_args() for Optuna arbitrary CLI params (ss-pkl)
- parse ODE and alpha params from solver stderr and record to DB
- add CLI flags for all ODE parameters (--beta, --gamma, --delta, --epsilon, --alpha-initial, --alpha-up-mult, --alpha-down-mult, --alpha-interval)
- campaign YAML parser and validator (scripts/campaign_config.py)
- idempotent benchmarks.db schema migration for Optuna columns
- pass solver args through cloud benchmark path

### Fixed

- emit restart_mode in solver config line for benchmark recording
- solver-args uses single string to avoid argparse flag collision

### Other

- apply alignment/review fixes to design doc (ss-mol-25sb)
- design document for Optuna experiment framework (ss-mol-25sb)
- PRD review findings and human clarifications (ss-mol-25sb)
- draft PRD for Optuna experiment framework (ss-mol-25sb)
- document musl binary rebuild requirement for cloud benchmarks

## [0.5.2](https://github.com/seanbearden/SpinSAT/compare/v0.5.1...v0.5.2) - 2026-03-25

### Added

- cloud worker captures all diagnostic fields from solver stderr
- track solved_by (dmm/cadical/preprocessing) and cdcl_handoffs per result
- dashboard rewrite with Explorer tab, run drill-down, and PAR-2 context
- split competition data into archive DB for size discipline
- benchmark script captures new metadata fields and structured tags
- schema migration — add columns, tables, and backfill data (ss-34v)
- emit peak_xl_max and final_dt diagnostics on stderr

### Fixed

- dashboard gracefully handles pre-migration DB schema
- add server-side VM timeout and broaden cleanup to all spinsat VMs
- auto-save uncommitted implementation work (ss-zvs, gt-pvx safety net)
- SSH poll loop reliability — single call, proper timeouts, max retries

### Other

- update CLAUDE.md benchmarking section with new schema and workflow
- tune restart params — xl_decay=0.5, noise=0.05
- update Serena memories with mixed batch test results (7/10 solved)

## [0.5.1](https://github.com/seanbearden/SpinSAT/compare/v0.5.0...v0.5.1) - 2026-03-23

### Added

- make cycling the default restart mode
- add restart mode A/B comparison script
- adaptive DMM/CaDiCaL time budget with confidence decay
- transfer DMM clause difficulty (x_l) to CaDiCaL via frustrated variable assumptions
- add sparse matrix derivative engine (challenger)
- add warm restart modes — voltage saving, x_l decay, anti-phase

### Fixed

- pass x_l clause difficulty to CaDiCaL in fallback path
- disable semver-checks for pre-1.0 releases
- add conflict limit to CDCL fallback to prevent unbounded solving
- use cross for musl static binary build
- use musl-cross toolchain for static binary build
- add workflow_dispatch to release workflow for manual binary builds
- provide C++ compiler for musl static binary build

### Other

- tune adaptive budget to solve 5/5 on competition UNSAT instance
- add hybrid DMM-CaDiCaL benchmark results to Serena memories
- fix 5 compiler warnings

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
