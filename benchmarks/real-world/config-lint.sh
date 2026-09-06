#!/usr/bin/env bash
# Simulated configuration linter.
#
# Models a CI-side validation tool: for each config entry, a fresh getopts
# pass parses that entry's option string, followed by declaration-heavy
# checks and formatted reporting. Deterministic output.
#
# Usage: config-lint.sh [count]
#   count   number of config entries to validate (default 250)

set -eu

COUNT=${1:-250}

total=0
warnings=0
errors=0
strict=no
prefix="."

validate() {
    local id=$1
    shift

    local path=$prefix scope=global mode=read-only verbose=no bad=0
    local opt err=0

    while getopts ":p:s:m:v" opt "$@"; do
        case "$opt" in
            p) path=$OPTARG ;;
            s) scope=$OPTARG ;;
            m) mode=$OPTARG ;;
            v) verbose=yes ;;
            \?) err=1 ; break ;;
        esac
    done

    # Validation rules (pure builtin work: patterns, substring ops, arithmetic).
    case "$scope" in
        global|user|session) ;;
        *) bad=$((bad + 1)) ;;
    esac
    case "$mode" in
        read-only|read-write) ;;
        *) bad=$((bad + 1)) ;;
    esac
    if [ "${#path}" -gt 64 ]; then
        bad=$((bad + 1))
    fi
    if [ "$((id % 7))" -eq 0 ]; then
        warnings=$((warnings + 1))
    fi

    if [ "$verbose" = yes ]; then
        printf 'entry %03d: path=%s scope=%-8s mode=%-10s\n' \
            "$id" "$path" "$scope" "$mode"
    fi

    if [ "$bad" -gt 0 ]; then
        errors=$((errors + bad))
        printf 'entry %03d: %d problem(s)\n' "$id" "$bad"
    fi

    total=$((total + 1))
}

i=0
while [ "$i" -lt "$COUNT" ]; do
    i=$((i + 1))
    case $((i % 3)) in
        0) validate "$i" -p "/etc/app/svc$i.conf" -s user -m read-write -v ;;
        1) validate "$i" -p "/var/lib/app/$i.db" -m read-only ;;
        2) validate "$i" -s session -m read-write -v ;;
    esac
done

printf 'checked %d entries: %d warning(s), %d error(s)\n' \
    "$COUNT" "$warnings" "$errors"
[ "$errors" -eq 0 ]
