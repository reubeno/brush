#!/bin/bash
set -euo pipefail

curl https://sh.rustup.rs -sSf | sh -s -- -y
rustup component add llvm-tools-preview
cargo install --locked cargo-audit
cargo install --locked cargo-deny
cargo install --locked cargo-llvm-cov
