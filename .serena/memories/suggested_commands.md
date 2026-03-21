# Suggested Commands

## Build
```bash
cargo build --release
```

## Run
```bash
./target/release/spinsat <instance.cnf>
```

## Test
```bash
cargo test
```

## Lint & Format
```bash
cargo fmt
cargo clippy
```

## Cross-compile for competition (static Linux binary)
```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

## Test in competition Docker image
```bash
podman pull registry.gitlab.com/sosy-lab/benchmarking/competition-scripts/user:latest
podman run --rm -it --volume=$(pwd):/tool --workdir=/tool \
  registry.gitlab.com/sosy-lab/benchmarking/competition-scripts/user:latest bash
./build.sh
./run.sh /path/to/instance.cnf /tmp/output/
```

## Download test instances
SAT Competition benchmarks available at: https://satcompetition.github.io/
Local test instances in: ~/Documents/DiVentraGroup/Factorization/Spin_SAT/uf100-430/

## System utils (macOS/Darwin)
- git, ls, cd, grep, find — standard unix
- brew for package management
- stat -f "%m" for file timestamps (not stat -c)
