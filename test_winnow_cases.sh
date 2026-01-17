#!/bin/bash

# Test script to compare PEG vs Winnow parser behavior
# Usage: ./test_winnow_cases.sh [peg|winnow|both]

set -e

BRUSH_BIN="./target/debug/brush"
TEST_FILE="winnow_parser_test_cases.txt"

# Build brush if needed
if [ ! -f "$BRUSH_BIN" ]; then
    echo "Building brush..."
    cargo build --bin brush
fi

test_with_parser() {
    local parser_name="$1"
    local extra_args="$2"
    
    echo "========================================"
    echo "Testing with $parser_name parser"
    echo "========================================"
    
    # Test specific cases that are known to fail with winnow
    local test_cases=(
        "Array Index Assignment"
        "ANSI-C Quotes"
        "printf %f"
        "Arithmetic for"
        "for loop without in"
        "gettext style quotes"
    )
    
    for case_name in "${test_cases[@]}"; do
        echo ""
        echo "=== Testing: $case_name ==="
        
        # Extract the test case
        sed -n "/^=== $case_name /,/^=== /p" "$TEST_FILE" | 
        sed '1d;$d' |  # Remove the === lines
        while read -r line; do
            if [ -n "$line" ]; then
                echo "Running: $line"
                if [ "$parser_name" = "winnow" ]; then
                    cargo run --bin brush --features experimental-parser -- --experimental-parser -c "$line" 2>&1 || echo "FAILED: $line"
                else
                    cargo run --bin brush -- -c "$line" 2>&1 || echo "FAILED: $line"
                fi
            fi
        done
    done
}

if [ "$#" -eq 0 ] || [ "$1" = "both" ]; then
    echo "Testing with PEG parser (default):"
    test_with_parser "peg" ""
    
    echo ""
    echo "Testing with Winnow parser (experimental):"
    test_with_parser "winnow" "--features experimental-parser --experimental-parser"
elif [ "$1" = "peg" ]; then
    test_with_parser "peg" ""
elif [ "$1" = "winnow" ]; then
    test_with_parser "winnow" "--features experimental-parser --experimental-parser"
else
    echo "Usage: $0 [peg|winnow|both]"
    exit 1
fi

echo ""
echo "Test comparison complete!"