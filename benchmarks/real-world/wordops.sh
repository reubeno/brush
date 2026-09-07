#!/usr/bin/env bash
# String/pattern/array workloads: parameter expansion, [[ ]] tests, printf,
# arrays. Builtin-heavy but not option-parsing-heavy.

set -eu

words=(alpha Beta GAMMA delta epsilon zeta OMEGA theta)
out=""

for i in {1..400}; do
    for w in "${words[@]}"; do
        l=${w,,}
        u=${w^^}
        if [[ $l == a* ]]; then
            out+="$(printf '%s|%s|%d;' "$u" "$l" "${#w}")"
        elif [[ $l == *[et]* ]]; then
            out+="${w:0:2}=${#out},"
        fi
    done

    if ((i % 100 == 0)); then
        arr=("${out//;/ }")
        out+="${#arr[@]}#${arr[0]:0:3}."
    fi
done

n=${#out}
printf '%d %d\n' "$n" "$((n % 97))"
