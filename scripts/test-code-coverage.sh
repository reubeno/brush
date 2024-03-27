#!/usr/bin/bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)
workspace_root="$(realpath "${script_dir}/..")"

export CARGO_TARGET_DIR="${workspace_root}/target/cov"

cd "${workspace_root}"
source <(cargo llvm-cov show-env --export-prefix)

cargo llvm-cov clean --workspace

cargo test -- --show-output || true

cargo llvm-cov report --html