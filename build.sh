#!/bin/bash
# SpinSAT build script for SAT Competition 2026
# Attempts to build from source with Rust; falls back to pre-compiled binary.

set -e

# Check if pre-compiled binary exists and is executable
if [ -x bin/spinsat-linux-x86_64 ]; then
    echo "Pre-compiled binary found."
fi

# Try to build from source if Rust is available
if command -v cargo &> /dev/null; then
    echo "Rust toolchain found, building from source..."
    cargo build --release 2>&1
    cp target/release/spinsat bin/spinsat-linux-x86_64 2>/dev/null || true
    echo "Build complete."
else
    echo "Rust not available, using pre-compiled binary."
fi

# Verify binary exists
if [ ! -x bin/spinsat-linux-x86_64 ]; then
    echo "ERROR: No solver binary available."
    exit 1
fi

echo "SpinSAT ready."
