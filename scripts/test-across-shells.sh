#!/bin/bash
set -euo pipefail

# Script to test shell script execution across different shells with detailed tracing
# Usage: test-across-shells.sh --output-dir=DIR --oracle-shell=SHELL --test-shell=SHELL SCRIPT [ARGS...]

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

usage() {
    cat >&2 <<EOF
Usage: $0 [OPTIONS] [--] SCRIPT [ARGS...]

Execute a shell script under both an oracle shell and a test shell, capturing
stdout, stderr, and execution traces for comparison.

Required Options:
  --output-dir=DIR       Directory where output files will be placed
  --test-shell=SHELL     Path to the shell under test

Optional Options:
  --oracle-shell=SHELL   Path to the oracle shell (default: bash)

Arguments:
  --                     Stop processing options (useful if SCRIPT starts with -)
  SCRIPT                 Path to the shell script to execute
  ARGS...                Arguments to pass to the script (may include options like -v)

Output Structure:
  <output-dir>/
    oracle/
      stdout.txt         Standard output from oracle shell
      stderr.txt         Standard error from oracle shell
      trace.txt          Execution trace (set -x output)
      exit_code.txt      Exit code from oracle shell
    test/
      stdout.txt         Standard output from test shell
      stderr.txt         Standard error from test shell
      trace.txt          Execution trace (set -x output)
      exit_code.txt      Exit code from test shell

The script sets BASH_XTRACEFD=3 to redirect trace output to fd 3, enables
tracing with 'set -x', and sets PS4 to include script name and line number.
EOF
    exit 1
}

# Parse command-line arguments
output_dir=""
oracle_shell="bash"
test_shell=""
script_path=""
declare -a script_args=()
parsing_options=true

while [[ $# -gt 0 ]]; do
    if [[ "$parsing_options" == true ]]; then
        case "$1" in
            --output-dir=*)
                output_dir="${1#*=}"
                shift
                ;;
            --oracle-shell=*)
                oracle_shell="${1#*=}"
                shift
                ;;
            --test-shell=*)
                test_shell="${1#*=}"
                shift
                ;;
            --help|-h)
                usage
                ;;
            --)
                # Stop parsing options, everything after this is for the script
                parsing_options=false
                shift
                ;;
            -*)
                echo -e "${RED}Error: Unknown option: $1${NC}" >&2
                usage
                ;;
            *)
                # First non-option argument is the script path
                script_path="$1"
                parsing_options=false
                shift
                ;;
        esac
    else
        # We've stopped parsing options
        if [[ -z "$script_path" ]]; then
            script_path="$1"
        else
            script_args+=("$1")
        fi
        shift
    fi
done

# Validate required arguments
if [[ -z "$output_dir" ]]; then
    echo -e "${RED}Error: --output-dir is required${NC}" >&2
    usage
fi

if [[ -z "$test_shell" ]]; then
    echo -e "${RED}Error: --test-shell is required${NC}" >&2
    usage
fi

if [[ -z "$script_path" ]]; then
    echo -e "${RED}Error: SCRIPT path is required${NC}" >&2
    usage
fi

if [[ ! -f "$script_path" ]]; then
    echo -e "${RED}Error: Script file not found: $script_path${NC}" >&2
    exit 1
fi

# Verify shells exist and are executable
for shell_name in oracle_shell test_shell; do
    shell_path="${!shell_name}"
    if ! command -v "$shell_path" >/dev/null 2>&1; then
        echo -e "${RED}Error: Shell not found or not executable: $shell_path${NC}" >&2
        exit 1
    fi
done

# Create output directory structure
mkdir -p "$output_dir/oracle"
mkdir -p "$output_dir/test"

# Log the parsed configuration
echo -e "${BLUE}📋 Configuration:${NC}"
echo "  Oracle shell: $oracle_shell"
echo "  Test shell:   $test_shell"
echo "  Script:       $script_path"
if [[ ${#script_args[@]} -gt 0 ]]; then
    echo "  Arguments:    ${script_args[*]}"
else
    echo "  Arguments:    (none)"
fi
echo "  Output dir:   $output_dir"
echo ""

# Function to execute script under a given shell
execute_with_shell() {
    local shell_path="$1"
    local output_subdir="$2"
    local stdout_file="$output_dir/$output_subdir/stdout.txt"
    local stderr_file="$output_dir/$output_subdir/stderr.txt"
    local trace_file="$output_dir/$output_subdir/trace.txt"
    local exit_code_file="$output_dir/$output_subdir/exit_code.txt"
    
    # Execute the shell with:
    # - BASH_XTRACEFD=3 set in the environment
    # - PS4 set to include script name and line number
    # - fd 3 redirected to trace file
    # - -x flag to enable tracing
    # - stdout to stdout file
    # - stderr to stderr file
    env BASH_XTRACEFD=3 PS4='+${BASH_SOURCE:-}:${LINENO:-}: ' \
        "$shell_path" -x "$script_path" "${script_args[@]}" \
        >"$stdout_file" \
        2>"$stderr_file" \
        3>"$trace_file"
    
    local exit_code=$?
    echo "$exit_code" > "$exit_code_file"
    return $exit_code
}

# Execute under oracle shell
echo -e "${BLUE}🔍 Executing under oracle shell ($oracle_shell)...${NC}"
if execute_with_shell "$oracle_shell" "oracle"; then
    oracle_exit=0
    echo -e "${GREEN}✅ Oracle shell completed successfully (exit code: 0)${NC}"
else
    oracle_exit=$?
    echo -e "${YELLOW}⚠️  Oracle shell exited with code: $oracle_exit${NC}"
fi

# Execute under test shell
echo -e "${BLUE}🧪 Executing under test shell ($test_shell)...${NC}"
if execute_with_shell "$test_shell" "test"; then
    test_exit=0
    echo -e "${GREEN}✅ Test shell completed successfully (exit code: 0)${NC}"
else
    test_exit=$?
    echo -e "${YELLOW}⚠️  Test shell exited with code: $test_exit${NC}"
fi

# Report results
echo ""
echo -e "📁 Execution complete. Output written to: $output_dir"
echo "  Oracle shell exit code: $oracle_exit"
echo "  Test shell exit code:   $test_exit"
echo ""

if [[ $oracle_exit -eq $test_exit ]]; then
    echo -e "${GREEN}✅ Exit codes match ✓${NC}"
else
    echo -e "${RED}❌ Exit codes differ ✗${NC}"
fi

# Exit with non-zero if the test results differ
if [[ $oracle_exit -ne $test_exit ]]; then
    exit 1
fi

exit 0
