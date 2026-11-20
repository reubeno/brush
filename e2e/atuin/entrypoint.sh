#!/bin/bash
# Contract: $SHELL_UNDER_TEST names the shell binary; results go to /results
# (junit XML + full log); exit status = test status.
set -uo pipefail
source /e2e/lib/run_pytest.sh

# atuin's bash integration is bash-preexec: `atuin init bash` inlines a copy of it and
# registers into precmd_functions/preexec_functions. brush implements those hooks natively but
# behind an experimental flag, so without it this suite only ever exercises bash-preexec's own
# DEBUG-trap emulation. Turn it on through brush's config file rather than a command-line flag:
# the suite takes a shell path with nowhere to put a flag, and bash ignores the file, so the
# `--baseline` run is unaffected. Remove once the feature is on by default.
mkdir -p "$HOME/.config/brush"
cat >"$HOME/.config/brush/config.toml" <<'TOML'
[experimental]
zsh-hooks = true
TOML

# atuin's suite has its own knob for the shell to test, so it needs no PATH shim (the shim
# would also capture tmux, which must not run under the shell under test).
export ATUIN_TEST_BASH="${SHELL_UNDER_TEST:-bash}"
e2e_run_pytest atuin "$@"
