# Competition Submission Status (2026-03-21)

## Ready Components
- Static Linux binary: bin/spinsat-linux-x86_64 (593KB, x86_64-unknown-linux-musl)
- build.sh: tries cargo build, falls back to pre-compiled binary
- run.sh: exec bin/spinsat-linux-x86_64 --timeout 5000 $1
- 20 benchmark instances: benchmarks/submission/ (500-2000 vars, CDC-style)
- All 20 benchmarks verified correct by SpinSAT

## Cross-Compilation
- Requires: rustup target add x86_64-unknown-linux-musl
- Requires: brew install filosottile/musl-cross/musl-cross (macOS)
- Config: .cargo/config.toml sets linker to x86_64-linux-musl-gcc
- Command: cargo build --release --target x86_64-unknown-linux-musl

## NOT YET DONE
- System description document (1-2 pages, IEEE Proceedings style, PDF)
- Registration email to organizers@satcompetition.org
- Test in competition Docker image (podman pull registry.gitlab.com/sosy-lab/...)
- Make repository public after submission deadline

## Key Dates
- April 19: Registration + benchmarks
- April 26: Solver code
- May 17: System description document
