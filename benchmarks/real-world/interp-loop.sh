#!/usr/bin/env bash
# Pure-interpretation loop: assignments and arithmetic, no external commands,
# no builtin argument parsing. Isolates interpreter cost from parser cost.

set -eu

x=0
for ((i = 0; i < 60000; i++)); do
    x=$((x + i % 7))
done

printf '%d\n' "$x"
