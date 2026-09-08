#!/bin/bash
# Contract: $SHELL_UNDER_TEST names the shell binary; results go to /results
# (junit XML + full log); exit status = test status.
set -uo pipefail
source /e2e/lib/run_pytest.sh
# ble.sh --test runs under whatever shell invokes it, so no PATH shim is needed.
export BLESH_TEST_SHELL="${SHELL_UNDER_TEST:-bash}"
# Arguments select test sections rather than pytest node ids, so they do not go to pytest.
export BLESH_TEST_FILES=
(($# == 0)) || printf -v BLESH_TEST_FILES '%s\n' "$@"
mkdir -p /results/progress
mkdir -p -m 700 /results/run
export XDG_RUNTIME_DIR=/results/run RUST_BACKTRACE=1
e2e_run_pytest blesh --tb=short
