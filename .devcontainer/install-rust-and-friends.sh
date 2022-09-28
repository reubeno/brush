#!/bin/bash
set -euo pipefail

curl https://sh.rustup.rs -sSf | sh -s -- -y
cargo install --locked cargo-audit
cargo install --locked cargo-deny
