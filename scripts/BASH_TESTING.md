# Bash Compatibility Testing

This directory contains tools for running the official bash test suite against brush to measure compatibility.

## Prerequisites

1. **Bash source code**: Download and extract bash source (e.g., bash-5.3)
   ```bash
   wget https://ftp.gnu.org/gnu/bash/bash-5.3.tar.gz
   tar xzf bash-5.3.tar.gz
   ```

2. **Build brush**: Build brush in debug mode (or release mode for performance)
   ```bash
   cargo build
   ```

## Tools

### 1. `bash-test-runner.py` - Individual Test Runner

Enhanced test runner with timeout protection, pass/fail tracking, detailed reporting, and **cargo nextest-style progress indicators**.

**Features:**
- Per-test timeout protection (prevents hanging tests)
- Automatic pass/fail detection by comparing outputs
- JSON export for analysis
- **Real-time progress indicators** showing test execution and pass/fail counts
- Verbose progress reporting
- Filter tests by name
- **Multi-suite support** - run multiple suites in one command

**Usage:**

```bash
# List all available tests
./scripts/bash-test-runner.py -b ~/src/bash-5.3 -s target/debug/brush list

# List available suites
./scripts/bash-test-runner.py -b ~/src/bash-5.3 -s target/debug/brush list --suites

# Run a single test
./scripts/bash-test-runner.py -b ~/src/bash-5.3 -s target/debug/brush test alias

# Run a single test suite
./scripts/bash-test-runner.py -b ~/src/bash-5.3 -s target/debug/brush suite minimal

# Run MULTIPLE test suites (new!)
./scripts/bash-test-runner.py -b ~/src/bash-5.3 -s target/debug/brush \
  suite minimal arith quote tilde

# Run suite with custom timeout (default: 30s)
./scripts/bash-test-runner.py -b ~/src/bash-5.3 -s target/debug/brush -t 60 suite minimal

# Filter tests in a suite
./scripts/bash-test-runner.py -b ~/src/bash-5.3 -s target/debug/brush suite all -f "array"

# Save results to JSON (single suite)
./scripts/bash-test-runner.py -b ~/src/bash-5.3 -s target/debug/brush \
  suite minimal --json results.json

# Save results for multiple suites to directory
./scripts/bash-test-runner.py -b ~/src/bash-5.3 -s target/debug/brush \
  suite minimal arith quote -o test-results/

# Verbose mode for detailed output
./scripts/bash-test-runner.py -b ~/src/bash-5.3 -s target/debug/brush -v suite minimal
```

**Output:**
- **Live progress indicators** with colors (e.g., `[12/25] ✓ 3 ✗ 8 Running posix2`)
- Colorized summary with pass rates
- Color-coded test status (green for pass, red for fail, yellow for timeout/error)
- List of failed/timeout/error tests
- Per-test timing information
- Aggregate summary when running multiple suites
- Optional JSON export with full details

> **Note:** Colors are automatically disabled when output is not a TTY or when `NO_COLOR` environment variable is set.

### 2. `compare-bash-test-results.py` - Results Comparison

Compare test results across runs to track progress or regressions.

**Usage:**

```bash
# Compare two test runs
./scripts/compare-bash-test-results.py compare baseline.json current.json

# Summary of multiple runs
./scripts/compare-bash-test-results.py summary \
  results1.json results2.json results3.json \
  --names "v1.0" "v1.1" "v1.2"
```

**Output:**
- Side-by-side comparison of metrics
- Lists of fixed/regressed tests
- Pass rate changes

### 3. `analyze-bash-test-results.py` - Results Analysis

Analyze test results to identify patterns and get recommendations.

**Usage:**

```bash
# Analyze results with insights
./scripts/analyze-bash-test-results.py results.json

# Show full error details
./scripts/analyze-bash-test-results.py results.json --show-errors

# Filter by status
./scripts/analyze-bash-test-results.py results.json --filter-status pass
./scripts/analyze-bash-test-results.py results.json --filter-status fail
```

**Output:**
- Categorized test lists (passing, failing, timeout, error)
- Statistics and pass rates
- Duration analysis
- Recommendations for next steps

## Common Workflows

### Initial Baseline

Establish a baseline of current compatibility:

```bash
# Run minimal suite
./scripts/bash-test-runner.py -b ~/src/bash-5.3 -s target/debug/brush -v \
  suite minimal --json baseline-minimal.json

# Or run all basic suites
./scripts/batch-bash-tests.py -b ~/src/bash-5.3 -s target/debug/brush \
  --suites minimal arith quote tilde -o baseline-results
```

### Track Progress

After making changes, compare against baseline:

```bash
# Run tests again
./scripts/bash-test-runner.py -b ~/src/bash-5.3 -s target/debug/brush -v \
  suite minimal --json current-minimal.json

# Compare
./scripts/compare-bash-test-results.py compare \
  baseline-minimal.json current-minimal.json
```

### Debug Failing Test

```bash
# Run single test with verbose output
./scripts/bash-test-runner.py -b ~/src/bash-5.3 -s target/debug/brush \
  test alias --show-output

# Or use the original wrapper for more control
./scripts/run-bash-tests.py -b ~/src/bash-5.3 -s target/debug/brush \
  diff --oracle bash alias
```

### Comprehensive Test Run

Run many suites to get comprehensive coverage:

```bash
./scripts/batch-bash-tests.py -b ~/src/bash-5.3 -s target/debug/brush \
  --suites minimal arith array quote tilde func heredoc \
  -o comprehensive-results
```

## Test Suites

The bash source contains many test suites. Key ones:

- **minimal**: Core POSIX-compatible features (~25 tests)
- **all**: Complete test suite (83 tests)
- **arith**: Arithmetic expansion
- **array**: Array operations
- **quote**: Quoting and escaping
- **tilde**: Tilde expansion
- **func**: Functions
- **heredoc**: Here documents
- **ifs**: IFS (field splitting)
- **read**: Read builtin
- Plus many more...

Run `./scripts/bash-test-runner.py -b ~/src/bash-5.3 -s target/debug/brush list --suites` to see all available suites.

## Handling Hanging Tests

The test runner includes timeout protection. If tests consistently timeout:

1. Increase timeout: `-t 60` (60 seconds)
2. Run individual test to debug: `test <name>`
3. Consider excluding from suite if it's a known limitation

## Understanding Results

**Pass**: Test output exactly matches expected output
**Fail**: Test output differs from expected
**Timeout**: Test exceeded time limit (may indicate infinite loop or deadlock)
**Error**: Test runner encountered an error (e.g., crash)

## Tips

1. **Start with minimal suite**: It's the most stable baseline
2. **Use verbose mode**: `-v` helps track progress on long runs
3. **Save JSON results**: Enables comparison and historical tracking
4. **Filter tests**: Use `-f` to focus on specific areas
5. **Adjust timeouts**: Some tests may legitimately take longer

## Integration with CI/CD

Example GitHub Actions workflow snippet:

```yaml
- name: Run bash compatibility tests
  run: |
    wget https://ftp.gnu.org/gnu/bash/bash-5.3.tar.gz
    tar xzf bash-5.3.tar.gz
    cargo build
    ./scripts/bash-test-runner.py -b bash-5.3 -s target/debug/brush \
      suite minimal --json test-results.json
    
- name: Upload results
  uses: actions/upload-artifact@v3
  with:
    name: bash-test-results
    path: test-results.json
```

## Troubleshooting

**"Shell not found" error**: Ensure the shell path is correct and brush is built
**"Tests directory not found"**: Check bash source directory path
**Many tests fail**: This is expected - brush is still achieving bash compatibility
**Tests timeout**: Increase timeout with `-t` flag or debug individual test
