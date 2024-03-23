#!/usr/bin/bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)
workspace_root="$(realpath "${script_dir}/..")"

cd "${workspace_root}"
source <(cargo llvm-cov show-env --export-prefix)

cargo llvm-cov clean --workspace

cargo build

cargo test -- --show-output

cargo llvm-cov report --html
