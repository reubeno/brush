#!/usr/bin/env bash
# Differential harness for comparing brush's argument-parsing behavior between
# two builds of the shell (e.g., a clap-based build and a usage-based build).
#
# Usage:
#   scripts/parser-parity.sh <reference-binary> <candidate-binary>
#
# Each corpus entry is a command string executed via `brush -c`. The candidate
# must match the reference on stdout and exit status. Stderr text is *not*
# compared: diagnostic wording differs by design between parsers.
#
# Exit code: 0 if all cases match, 1 otherwise.

set -u

REF="${1:?usage: parser-parity.sh <reference-binary> <candidate-binary>}"
CAND="${2:?usage: parser-parity.sh <reference-binary> <candidate-binary>}"

CORPUS=(
    # set ±o handling (bare listing, accumulation, attached forms, odd values)
    'set -o nounset -o xtrace; echo rc=$?'
    'set +o posix +o allexport; echo rc=$?'
    'set -oa -ob; echo rc=$?'
    'set -eo; echo rc=$?'
    'set -ox; echo rc=$?'
    'set -o | head -3'
    'set +o | head -3'
    'set -o ""; echo rc=$?'
    'set -o -x; echo rc=$?'
    'set +o -x; echo rc=$?'
    'set -- -o; echo "pos=$1 rc=$?"'
    'set -o posix; set +o | grep posix'
    # flag-like operands that must not be treated as options
    'echo -B:'
    'printf "%s\n" -5'
    'printf "%d %d\n" -1 -2'
    'printf -- "%s\n" -- x'
    'printf "%s|%s\n" a -- b'
    'let "x=-1+2"; echo $x'
    'test -n -1; echo rc=$?'
    # `--` and option-terminator semantics
    'echo -- hello'
)

fail=0
pass=0

run_case() {
    local desc="$1"
    local ref_out ref_rc cand_out cand_rc

    ref_out=$("$REF" -c "$desc" 2>/dev/null)
    ref_rc=$?

    cand_out=$("$CAND" -c "$desc" 2>/dev/null)
    cand_rc=$?

    if [ "$ref_out" = "$cand_out" ] && [ "$ref_rc" = "$cand_rc" ]; then
        pass=$((pass + 1))
        echo "PASS: $desc"
    else
        fail=$((fail + 1))
        echo "FAIL: $desc (ref rc=$ref_rc, cand rc=$cand_rc)"
        diff <(printf '%s\n' "$ref_out") <(printf '%s\n' "$cand_out") | head -10
    fi
}

for case in "${CORPUS[@]}"; do
    run_case "$case"
done

echo
echo "parser parity: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
