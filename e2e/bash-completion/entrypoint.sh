#!/bin/bash
# Contract: $SHELL_UNDER_TEST names the shell binary; results go to /results
# (junit XML + full log); exit status = test status.
set -uo pipefail
source /e2e/lib/run_pytest.sh
# The runner's host uid has no account here, which user and group completions (chown, compgen -u)
# and the suite itself (getpwuid) need.
if ! getent passwd "$(id -u)" >/dev/null; then
    echo "e2e:x:$(id -u):$(id -g)::${HOME}:/bin/bash" >>/etc/passwd
    getent group "$(id -g)" >/dev/null || echo "e2e:x:$(id -g):" >>/etc/group
fi
# The suite takes a command line, not a path, so brush's own flags can go here. Unset, the suite
# runs the image's bash, which is the baseline.
case ${SHELL_UNDER_TEST-} in
    '') ;;
    */brush*) export BASH_COMPLETION_TEST_BASH="$SHELL_UNDER_TEST --noprofile --no-config --input-backend=basic" ;;
    *) export BASH_COMPLETION_TEST_BASH="$SHELL_UNDER_TEST --noprofile" ;;
esac
cd /bash-completion/test || exit 1
E2E_TESTS=t e2e_run_pytest bash-completion -n auto "$@"
