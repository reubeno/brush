#!/bin/bash
# Contract: $SHELL_UNDER_TEST names the shell binary; results go to /results
# (junit XML + full log); exit status = test status.
set -uo pipefail
source /e2e/lib/run_pytest.sh
# atuin's suite has its own knob for the shell to test, so it needs no PATH shim (the shim
# would also capture tmux, which must not run under the shell under test).
export ATUIN_TEST_BASH=${SHELL_UNDER_TEST:-bash}
e2e_run_pytest atuin "$@"
