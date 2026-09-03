#!/bin/bash
# Contract: $SHELL_UNDER_TEST names the shell binary (see ../shim/bash);
# results go to /results (junit XML + full log); exit status = test status.
set -uo pipefail
mkdir -p /results
cd /fzf
tmux new-session -d
# Turn skip-list.txt into a minitest --exclude regex.
skips=$(grep -v '^\s*\(#\|$\)' /e2e/skip-list.txt | paste -sd'|')
# Non-bash shells are excluded (rather than selected with -n) so callers can pass their own -n.
exclude="^(TestZsh|TestFish|TestNushell)#"
[[ -n $skips ]] && exclude="$exclude|^($skips)$"
ruby -Itest test/test_shell_integration.rb --verbose --exclude "/$exclude/" \
    --ci-report --ci-dir /results/junit "$@" 2>&1 | tee /results/log.txt
exit $?
