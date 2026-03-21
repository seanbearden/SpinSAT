# SAT Competition 2026 Requirements

## Submission Format
- Private GitHub repo (made public after deadline)
- Must contain: source code, build.sh, run.sh
- build.sh: takes no params, builds solver
- run.sh: $1 = CNF file path, $2 = proof output dir

## Runtime Environment
- Ubuntu 24.04 Docker image (SoSy-Lab)
- GCC, Clang, Python3, Java pre-installed
- Rust NOT pre-installed — must pre-compile or vendor
- NO guaranteed network access during build
- HoreKa Blue cluster: Intel Xeon Platinum 8368

## Constraints
- Time: 5000 seconds (CPU time)
- Memory: 32 GB
- Single-threaded (sequential track)
- PAR-2 scoring: unsolved = 2× timeout penalty

## Experimental Track (our target)
- No UNSAT proof certificates required
- Evaluated on NEW benchmark instances only
- Must outperform top 3 Main Track solvers on new instances for award
- Perfect for incomplete/unconventional solvers

## Input: DIMACS CNF
- Lines starting with 'c' = comments
- Header: p cnf <num_vars> <num_clauses>
- Clauses: space-separated literals terminated by 0
- Positive int = variable, negative = negation

## Output (stdout)
- s SATISFIABLE / s UNSATISFIABLE / s UNKNOWN
- v <literals> ... 0 (if SAT)
- Max 4096 chars per value line

## Build Strategy for Rust
Pre-compile static binary (x86_64-unknown-linux-musl) and include in repo.
build.sh: echo "pre-compiled" or try cargo build with vendored deps as fallback.

## Key Dates
- April 19: Registration + benchmarks
- April 26: Solver code
- May 17: System description document
