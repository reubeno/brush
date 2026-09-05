#!/bin/bash
# Contract: $SHELL_UNDER_TEST names the shell binary; results go to /results
# (junit XML + full log); exit status = test status.
set -uo pipefail
# The suite takes the bash to test directly, so no PATH shim is needed (and bats, which
# is itself a bash script, must not run under the shell under test).
export ATUIN_TEST_BASH=${SHELL_UNDER_TEST:-bash}
mkdir -p /results/junit
# The suite's setup() skips any test whose name is listed here (see tests/helpers/shell.bash).
ATUIN_TEST_SKIP=$(grep -v '^\s*\(#\|$\)' /e2e/skip-list.txt)
export ATUIN_TEST_SKIP
bats --report-formatter junit --output /results/junit --print-output-on-failure --timing \
    "$@" /e2e/tests 2>&1 | tee /results/log.txt
exit $?
