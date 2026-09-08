#!/bin/bash
# Contract: $SHELL_UNDER_TEST names the shell binary; results go to /results
# (junit XML + full log); exit status = test status.
set -uo pipefail
source /e2e/lib/run_pytest.sh
export STARSHIP_TEST_SHELL=${SHELL_UNDER_TEST:-bash}
e2e_run_pytest starship "$@"
