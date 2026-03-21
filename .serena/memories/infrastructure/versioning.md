# Versioning & Release Infrastructure (2026-03-21)

## Automated Versioning: release-plz
- **Tool**: release-plz (Rust-native, zero config)
- **Workflow**: `.github/workflows/release-plz.yml`
- **How it works**: Push to main → release-plz opens Release PR with auto version bump + CHANGELOG → merge PR → git tag + GitHub Release + crates.io publish
- **No conventional commits required**: all non-conventional commits are patch bumps. Optional `feat:` prefix for minor bumps.
- **CHANGELOG**: auto-generated via git-cliff (built into release-plz)
- **Config**: none needed — reads from Cargo.toml

## Version Source of Truth
- `Cargo.toml` version field (single source)
- `spinsat --version` reads via `env!("CARGO_PKG_VERSION")` — no hardcoded strings
- `-V` / `--version` flag added to CLI

## Release Binary
- **Workflow**: `.github/workflows/release-binary.yml`
- **Trigger**: `release: published` (fires after release-plz creates a release)
- **Output**: static binary `x86_64-unknown-linux-musl` attached to GitHub Release
- **benchmarks.db**: uploaded manually via `gh release upload <tag> benchmarks.db` when data is meaningful

## GitHub Repo Settings Required
- Actions → Workflow permissions → Read and write
- Actions → Allow GitHub Actions to create and approve pull requests (checked)
- Secrets: `CARGO_REGISTRY_TOKEN` (crates.io, scoped to spinsat crate, publish-new + publish-update)

## Current Version
- v0.4.0 (synced from previous hardcoded string)

## Distribution
- **GitHub Releases**: primary (binary + benchmarks.db)
- **crates.io**: automatic via release-plz
- **Homebrew**: deferred (no audience yet, trivial to add later)
