#!/usr/bin/env bash
#
# Quick status check for bash compatibility testing.
# Runs a fast subset of tests and reports pass rate.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASH_SOURCE_DIR="${BASH_SOURCE_DIR:-$HOME/src/bash-5.3}"
SHELL="${SHELL_PATH:-target/debug/brush}"

# Check if bash source exists
if [ ! -d "$BASH_SOURCE_DIR" ]; then
    echo "Error: Bash source directory not found: $BASH_SOURCE_DIR"
    echo "Set BASH_SOURCE_DIR environment variable or ensure ~/src/bash-5.3 exists"
    exit 1
fi

# Check if shell exists
if [ ! -f "$SHELL" ]; then
    echo "Error: Shell not found: $SHELL"
    echo "Build brush with: cargo build"
    exit 1
fi

echo "Bash Compatibility Quick Check"
echo "==============================="
echo "Bash source: $BASH_SOURCE_DIR"
echo "Testing shell: $SHELL"
echo ""

# Run quick tests (minimal suite with short timeout)
python3 "$SCRIPT_DIR/bash-test-runner.py" \
    -b "$BASH_SOURCE_DIR" \
    -s "$SHELL" \
    -t 15 \
    suite minimal

exit_code=$?

echo ""
if [ $exit_code -eq 0 ]; then
    echo "✓ All tests passed!"
else
    echo "✗ Some tests failed (see above for details)"
fi

exit $exit_code
