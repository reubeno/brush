#!/bin/bash
# Contract: $SHELL_UNDER_TEST names the shell binary; results go to /results
# (junit XML + full log); exit status = test status.
set -uo pipefail
source /e2e/lib/run_pytest.sh
# The baseline is the bash built alongside the suite, not the image's /bin/bash.
export THIS_SH="${SHELL_UNDER_TEST:-/bash/bash}"
e2e_run_pytest bash-tests -n auto "$@"
