#!/bin/bash
set -euo pipefail

# Install rustup and needed components
# Reference: https://rustup.rs/
curl https://sh.rustup.rs -sSf | sh -s -- -y
rustup component add llvm-tools-preview

# Install cargo binstall
# Reference: https://github.com/cargo-bins/cargo-binstall
curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

# Install cargo tools using binstall
cargo binstall --no-confirm cargo-audit
cargo binstall --no-confirm cargo-deny
# cargo binstall --no-confirm cargo-flamegraph
cargo binstall --no-confirm cargo-llvm-cov
