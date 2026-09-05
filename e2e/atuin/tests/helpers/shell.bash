# Helpers for driving an interactive shell with the atuin integration inside tmux.
# Sourced by the per-shell .bats files via `load helpers/shell`.

: "${ATUIN_TEST_BASH:=bash}"
: "${ATUIN_TEST_TIMEOUT:=10}"

# bats hooks -------------------------------------------------------------

setup() {
    if [[ -n ${ATUIN_TEST_SKIP-} ]] && grep -qxF -- "$BATS_TEST_DESCRIPTION" <<<"$ATUIN_TEST_SKIP"; then
        skip "listed in ATUIN_TEST_SKIP"
    fi

    # Isolate atuin's config and database per test.
    export HOME=$BATS_TEST_TMPDIR/home
    mkdir -p "$HOME"
    unset XDG_CONFIG_HOME XDG_DATA_HOME ATUIN_SESSION ATUIN_HISTORY_ID

    # Create and migrate the database once, up front: atuin migrates on first use, and two
    # atuin processes racing to migrate a fresh database trip a UNIQUE constraint. Our
    # out-of-band history queries would otherwise race the shell's own atuin at startup.
    ATUIN_SESSION=$(atuin uuid) atuin history list >/dev/null 2>&1 || true

    export TMUX_SOCKET=$BATS_TEST_TMPDIR/tmux
    export TMUX_TMPDIR=$BATS_TEST_TMPDIR
    unset TMUX
}

teardown() {
    tmux -S "$TMUX_SOCKET" kill-server 2>/dev/null || true
}

# Shell lifecycle --------------------------------------------------------

# Starts an interactive shell with `atuin init <shell>` loaded. Waits for the prompt.
start_shell() {
    local shell=${1:-bash} rc=$BATS_TEST_TMPDIR/rc
    case $shell in
        bash)
            printf 'PS1="atuin-test> "\nHISTFILE=\neval "$(atuin init bash)"\n' >"$rc"
            tmux -S "$TMUX_SOCKET" new-session -d -x 100 -y 30 "$ATUIN_TEST_BASH --noprofile --rcfile $rc"
            ;;
        *) echo "unsupported shell: $shell" >&2; return 1 ;;
    esac
    wait_for_prompt
}

# Sends keys as tmux understands them (e.g. `Enter`, `C-r`, `Up`, or literal text).
send_keys() {
    tmux -S "$TMUX_SOCKET" send-keys "$@"
}

# Types a command line and runs it.
run_line() {
    send_keys -l "$1"
    send_keys Enter
}

# Prints the screen, trailing blank lines removed.
screen() {
    tmux -S "$TMUX_SOCKET" capture-pane -p | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}'
}

# Waits until the screen matches the extended regex, or fails with the screen contents.
wait_for() {
    local pattern=$1 deadline=$((SECONDS + ATUIN_TEST_TIMEOUT))
    until screen | grep -qE -- "$pattern"; do
        if ((SECONDS >= deadline)); then
            echo "timed out waiting for /$pattern/; screen:" >&2
            screen >&2
            return 1
        fi
        sleep 0.1
    done
}

# Waits for the shell to show a prompt as the last line.
wait_for_prompt() {
    local deadline=$((SECONDS + ATUIN_TEST_TIMEOUT))
    until [[ $(screen | tail -n 1) == "atuin-test>" ]]; do
        if ((SECONDS >= deadline)); then
            echo "timed out waiting for prompt; screen:" >&2
            screen >&2
            return 1
        fi
        sleep 0.1
    done
}

# Waits until exactly N screen lines match the extended regex; shows the screen otherwise.
wait_for_screen_count() {
    local pattern=$1 expected=$2 deadline=$((SECONDS + ATUIN_TEST_TIMEOUT)) actual
    while :; do
        actual=$(screen | grep -cE -- "$pattern")
        ((actual == expected)) && return 0
        if ((SECONDS >= deadline || actual > expected)); then
            echo "expected $expected line(s) matching /$pattern/, found $actual; screen:" >&2
            screen >&2
            return 1
        fi
        sleep 0.1
    done
}

# Runs an empty command so atuin's precmd fires `history end` for the previous command,
# whose exit status it only records on the following prompt.
flush_history() {
    send_keys Enter
    wait_for_prompt
}

# History assertions -----------------------------------------------------

# Waits until `atuin history list` (run outside the shell, against the same database)
# matches the extended regex. atuin records `history end` asynchronously, so this polls.
wait_for_history() {
    local pattern=$1 deadline=$((SECONDS + ATUIN_TEST_TIMEOUT)) listing
    while :; do
        # atuin insists on a session id even just to list.
        listing=$(ATUIN_SESSION=${ATUIN_SESSION:-$(atuin uuid)} atuin history list --format '{exit} {command}' 2>/dev/null) || listing=""  # may race with db creation
        grep -qE -- "$pattern" <<<"$listing" && return 0
        if ((SECONDS >= deadline)); then
            echo "timed out waiting for history /$pattern/; history:" >&2
            echo "$listing" >&2
            return 1
        fi
        sleep 0.1
    done
}
