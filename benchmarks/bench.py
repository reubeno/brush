#!/usr/bin/env python3
"""
Shell benchmarking harness for brush.

Runs shell script snippets against test and reference shells using
warmup, calibration, and timed execution phases.

NOTE: This script was authored by an AI coding agent in conjunction
      with human guidance.
"""

import argparse
import json
import math
import os
import re
import shutil
import statistics
import subprocess
import sys
from dataclasses import dataclass
from typing import Optional


@dataclass
class BenchmarkCase:
    """
    A benchmark case measuring a specific shell operation.

    The benchmark runs `loop_body` in a tight loop, with optional `setup`
    executed once before timing and `cleanup` after.

    Exit-code contract:
        - The `setup` phase may validate preconditions and exit with non-zero
          status to signal failure (e.g., missing external commands).
        - The generated script always ends with `exit 0` to confirm success.
        - Any non-zero exit from the shell is treated as a fatal benchmark error.
    """

    loop_body: str
    setup: Optional[str] = None
    cleanup: Optional[str] = None


@dataclass
class TimingResult:
    """
    Timing data parsed from shell's `time` builtin output.

    Captures wall-clock time (real), user CPU time, and system CPU time.
    """

    real_seconds: float
    user_seconds: float
    sys_seconds: float


@dataclass
class BenchmarkResult:
    """
    Aggregated results from running a benchmark across multiple samples.

    Contains per-iteration timing statistics (median, MAD, min, max) computed
    from `sample_count` independent timed runs. Uses median and MAD (median
    absolute deviation) instead of mean and stddev for robustness against
    outliers from system jitter.
    """

    case_name: str
    iterations_per_sample: int
    sample_count: int
    per_iteration_ns: float  # median across samples
    per_iteration_ns_mad: float  # MAD scaled by 1.4826 for normal comparability
    per_iteration_ns_min: float
    per_iteration_ns_max: float
    shell_path: str


@dataclass
class ComparisonResult:
    """
    Performance comparison between test shell and reference shell.

    The `change` ratio is reference_time / test_time, so values > 1.0
    indicate the test shell is faster than the reference.
    """

    case_name: str
    test_result: BenchmarkResult
    reference_result: BenchmarkResult
    change: float  # ratio: ref_time / test_time (>1 = test is faster)


# =============================================================================
# Built-in benchmark cases
# =============================================================================

BENCHMARK_CASES: dict[str, BenchmarkCase] = {
    "assignment": BenchmarkCase(
        loop_body="x=42",
    ),
    "colon": BenchmarkCase(
        loop_body=":",
    ),
    "echo_builtin": BenchmarkCase(
        loop_body="echo >/dev/null",
    ),
    "echo_cmd": BenchmarkCase(
        setup="command -v /usr/bin/echo >/dev/null || exit 1",
        loop_body="/usr/bin/echo >/dev/null",
    ),
    "increment": BenchmarkCase(
        setup="x=0",
        loop_body="((x++))",
    ),
    "subshell": BenchmarkCase(
        loop_body="(:)",
    ),
    "cmdsubst": BenchmarkCase(
        loop_body=': $(:)',
    ),
    "var_expand": BenchmarkCase(
        setup='myvar="hello world"',
        loop_body=': "$myvar"',
    ),
    "func_call": BenchmarkCase(
        setup="myfunc() { :; }",
        loop_body="myfunc",
    ),
    "array_access": BenchmarkCase(
        setup='myarr=(a b c d e f g h i j)',
        loop_body=': "${myarr[5]}"',
    ),
    "pattern_match": BenchmarkCase(
        loop_body='[[ "hello world" == hello* ]]',
    ),
    "regex_match": BenchmarkCase(
        loop_body='[[ "hello world" =~ ^hello.* ]]',
    ),
    "if_taken": BenchmarkCase(
        loop_body="if [[ 1 -eq 1 ]]; then :; fi",
    ),
    "if_not_taken": BenchmarkCase(
        loop_body="if [[ 0 -eq 1 ]]; then :; fi",
    ),
}


# =============================================================================
# Time output parsing
# =============================================================================


def parse_time_output(stderr: str) -> TimingResult:
    """
    Parse bash's default time output format.

    Expected format (on stderr):
        real    0m0.123s
        user    0m0.045s
        sys     0m0.012s

    The generated benchmark scripts explicitly set TIMEFORMAT to match bash's
    default format, ensuring consistent parsing across shells that support
    this variable. Locale settings (LC_NUMERIC) could theoretically affect
    decimal separators, but this is rare in practice.
    """
    # Pattern matches: real/user/sys followed by XmY.ZZZs format
    pattern = r"(real|user|sys)\s+(\d+)m([\d.]+)s"
    matches = re.findall(pattern, stderr)

    if len(matches) < 3:
        raise ValueError(f"Failed to parse time output: {stderr!r}")

    results = {}
    for label, minutes, seconds in matches:
        total_seconds = int(minutes) * 60 + float(seconds)
        results[label] = total_seconds

    return TimingResult(
        real_seconds=results.get("real", 0.0),
        user_seconds=results.get("user", 0.0),
        sys_seconds=results.get("sys", 0.0),
    )


# =============================================================================
# Script generation
# =============================================================================


def generate_benchmark_script(case: BenchmarkCase, iterations: int) -> str:
    """
    Generate a complete benchmark script for the given case.

    The script:
    1. Sets TIMEFORMAT to match bash's default output format
    2. Runs optional setup (which may exit non-zero to abort)
    3. Times the loop body for the specified iterations
    4. Runs optional cleanup
    5. Exits with 0 to confirm successful completion
    """
    lines = []

    # Pin TIMEFORMAT to bash's default format for consistent parsing
    # Format: real\t%lR\nuser\t%lU\nsys\t%lS (where %l uses Mm.SSSs format)
    lines.append(r"TIMEFORMAT=$'real\t%lR\nuser\t%lU\nsys\t%lS'")

    # Setup (optional) - may exit non-zero to signal precondition failure
    if case.setup:
        lines.append(case.setup)

    # Timed loop (no braces needed around for loop)
    lines.append(
        f"time for ((i=0; i<{iterations}; i++)); do {case.loop_body}; done"
    )

    # Cleanup (optional)
    if case.cleanup:
        lines.append(case.cleanup)

    # Explicit success exit to confirm script completed without error
    lines.append("exit 0")

    return "\n".join(lines)


# =============================================================================
# Shell execution
# =============================================================================


def get_shell_flags(shell_path: str) -> list[str]:
    """Get appropriate flags for running a shell in benchmark mode."""
    # Check if this looks like brush
    shell_name = shell_path.split("/")[-1]

    if "brush" in shell_name:
        return [
            "--norc",
            "--noprofile",
            "--input-backend=basic",
            "--disable-bracketed-paste",
            "--disable-color",
        ]
    else:
        # Assume bash-like shell
        return ["--norc", "--noprofile"]


def run_shell_script(shell_path: str, script: str) -> subprocess.CompletedProcess:
    """Run a script using the specified shell."""
    flags = get_shell_flags(shell_path)
    cmd = [shell_path] + flags + ["-c", script]

    return subprocess.run(
        cmd,
        capture_output=True,
        text=True,
    )


# =============================================================================
# Benchmark execution
# =============================================================================

# -----------------------------------------------------------------------------
# Unit conversion constants
# -----------------------------------------------------------------------------

# Nanoseconds per second, used for timing conversions.
NS_PER_SECOND = 1_000_000_000

# -----------------------------------------------------------------------------
# Warmup phase constants
# -----------------------------------------------------------------------------

# Default warmup duration in seconds. Warmup ensures CPU caches are hot and
# the shell's internal state is stable before measurement begins.
# Set to 0 to skip warmup entirely.
DEFAULT_WARMUP_DURATION_SECONDS = 0.5

# Number of iterations for the quick probe used to estimate per-iteration time
# during warmup. This should be small enough to complete quickly but large
# enough to get a rough timing estimate.
WARMUP_PROBE_ITERATIONS = 100

# Multiplier applied to WARMUP_PROBE_ITERATIONS when the probe completes too
# fast to measure (real_seconds == 0). This is a fallback for extremely fast
# operations where even 100 iterations complete in sub-millisecond time.
WARMUP_FALLBACK_MULTIPLIER = 10

# -----------------------------------------------------------------------------
# Calibration phase constants
# -----------------------------------------------------------------------------

# Starting iteration count for calibration. We double this until we get a
# measurable time (>= CALIBRATION_MIN_TIME_SECONDS).
CALIBRATION_MIN_ITERATIONS = 100

# Minimum elapsed time in seconds for reliable calibration measurement.
# Below this threshold, timer resolution and process startup overhead
# dominate the measurement.
CALIBRATION_MIN_TIME_SECONDS = 0.1

# Maximum iterations to attempt during calibration. Prevents runaway loops
# if something is fundamentally broken.
CALIBRATION_MAX_ITERATIONS = 100_000_000

# Maximum iterations per sample. Prevents extremely long-running samples
# even if per-iteration time is very small.
MAX_TARGET_ITERATIONS = 1_000_000_000

# -----------------------------------------------------------------------------
# Statistical sampling constants
# -----------------------------------------------------------------------------

# Default number of independent samples to collect. More samples improve
# statistical reliability but increase total benchmark time.
DEFAULT_SAMPLES = 10

# Minimum duration per sample in seconds. Samples shorter than this are
# unreliable due to timer resolution and system noise.
MIN_SAMPLE_DURATION_SECONDS = 0.5

# Scale factor to convert MAD (median absolute deviation) to an estimate
# comparable to standard deviation for normally distributed data.
# For a normal distribution, MAD * 1.4826 ≈ stddev.
MAD_NORMAL_SCALE_FACTOR = 1.4826

# Coefficient of variation threshold (as percentage) above which we flag
# results as having high variance, indicating unreliable measurements.
HIGH_VARIANCE_THRESHOLD_PCT = 10.0

# -----------------------------------------------------------------------------
# Performance comparison constants
# -----------------------------------------------------------------------------

# Default benchmark duration in seconds (total time budget per benchmark).
DEFAULT_DURATION_SECONDS = 10.0

# Threshold for classifying performance change as "faster" (test outperforms
# reference). A value of 1.1 means test must be at least 10% faster.
CHANGE_FASTER_THRESHOLD = 1.1

# Threshold for classifying performance change as "similar" (no significant
# difference). A value of 0.95 means test can be up to 5% slower and still
# be considered similar. Below this is classified as "slower".
CHANGE_SIMILAR_THRESHOLD = 0.95


def run_benchmark_phase(
    shell_path: str, case: BenchmarkCase, iterations: int
) -> TimingResult:
    """
    Run a single benchmark phase and return timing.

    Raises RuntimeError if the shell exits with non-zero status, which
    indicates either a setup precondition failure or an execution error.
    """
    script = generate_benchmark_script(case, iterations)
    result = run_shell_script(shell_path, script)

    if result.returncode != 0:
        raise RuntimeError(
            f"Benchmark failed (exit {result.returncode}). This may indicate "
            f"a setup precondition failure or shell execution error.\n"
            f"stdout: {result.stdout}\n"
            f"stderr: {result.stderr}"
        )

    return parse_time_output(result.stderr)


def run_benchmark(
    shell_path: str,
    case_name: str,
    case: BenchmarkCase,
    target_duration: float,
    num_samples: int = DEFAULT_SAMPLES,
    warmup_duration: float = DEFAULT_WARMUP_DURATION_SECONDS,
    verbose: bool = False,
) -> BenchmarkResult:
    """
    Run a complete benchmark with warmup, calibration, and multiple sample phases.

    Args:
        shell_path: Path to shell executable
        case_name: Name of the benchmark case
        case: Benchmark case to run
        target_duration: Total time budget in seconds (divided among samples)
        num_samples: Number of independent samples to collect
        warmup_duration: Time in seconds to spend warming up (default: 0.5s; 0 to skip)
        verbose: Print progress information

    Returns:
        BenchmarkResult with timing statistics (median, MAD) across all samples
    """
    # Determine actual sample count based on duration constraints
    sample_duration = target_duration / num_samples
    actual_samples = num_samples

    if sample_duration < MIN_SAMPLE_DURATION_SECONDS:
        actual_samples = max(1, int(target_duration / MIN_SAMPLE_DURATION_SECONDS))
        sample_duration = target_duration / actual_samples
        # Always warn when we reduce samples (even in non-verbose mode)
        print(
            f"  ⚠️  Duration too short for {num_samples} samples; "
            f"using {actual_samples} sample(s) of {sample_duration:.1f}s instead",
            file=sys.stderr,
        )

    # Phase 1: Time-based warmup (skip if warmup_duration is 0)
    if warmup_duration > 0:
        if verbose:
            print(f"  🔥 Warming up ({warmup_duration}s)...", file=sys.stderr)

        warmup_probe = run_benchmark_phase(shell_path, case, WARMUP_PROBE_ITERATIONS)
        warmup_per_iter = warmup_probe.real_seconds / WARMUP_PROBE_ITERATIONS
        if warmup_per_iter > 0:
            warmup_iterations = max(1, int(warmup_duration / warmup_per_iter))
        else:
            # Probe was too fast to measure - use fallback iteration count
            warmup_iterations = WARMUP_PROBE_ITERATIONS * WARMUP_FALLBACK_MULTIPLIER
            if verbose:
                print(
                    f"     (warmup probe too fast to measure, using {warmup_iterations} iterations)",
                    file=sys.stderr,
                )

        run_benchmark_phase(shell_path, case, warmup_iterations)
    elif verbose:
        print("  🔥 Skipping warmup (duration=0)", file=sys.stderr)

    if verbose:
        print("  📏 Calibrating...", file=sys.stderr)

    # Phase 2: Adaptive calibration
    # Keep doubling iterations until we get a measurable time
    calibration_iterations = CALIBRATION_MIN_ITERATIONS
    calibration_timing: Optional[TimingResult] = None
    while calibration_iterations <= CALIBRATION_MAX_ITERATIONS:
        calibration_timing = run_benchmark_phase(shell_path, case, calibration_iterations)

        if calibration_timing.real_seconds >= CALIBRATION_MIN_TIME_SECONDS:
            # We have enough time for reliable measurement
            break

        if verbose:
            print(
                f"     (calibration too fast: {calibration_timing.real_seconds:.4f}s "
                f"for {calibration_iterations} iters, doubling...)",
                file=sys.stderr,
            )

        calibration_iterations *= 2

    assert calibration_timing is not None  # Loop always runs at least once
    if calibration_timing.real_seconds < CALIBRATION_MIN_TIME_SECONDS:
        # Even at max iterations, still too fast - use what we have
        if verbose:
            print(
                f"     (warning: calibration capped at {calibration_iterations} iterations)",
                file=sys.stderr,
            )

    # Calculate iterations needed for each sample
    per_iteration_seconds = calibration_timing.real_seconds / calibration_iterations
    if per_iteration_seconds <= 0:
        # Fallback if calibration was too fast: use minimum measurable time
        # divided by max iterations as a conservative lower bound
        per_iteration_seconds = CALIBRATION_MIN_TIME_SECONDS / CALIBRATION_MAX_ITERATIONS

    iterations_per_sample = max(1, int(sample_duration / per_iteration_seconds))
    iterations_per_sample = min(iterations_per_sample, MAX_TARGET_ITERATIONS)

    if verbose:
        print(
            f"  ⏱️  Running {actual_samples} samples of {iterations_per_sample} iterations "
            f"(est. {sample_duration:.1f}s each)...",
            file=sys.stderr,
        )

    # Phase 3: Collect multiple samples
    sample_ns_values: list[float] = []
    for sample_idx in range(actual_samples):
        sample_timing = run_benchmark_phase(shell_path, case, iterations_per_sample)
        per_iter_ns = (sample_timing.real_seconds / iterations_per_sample) * NS_PER_SECOND
        sample_ns_values.append(per_iter_ns)

        if verbose:
            print(
                f"     Sample {sample_idx + 1}/{actual_samples}: "
                f"{format_duration_ns(per_iter_ns)}/iter",
                file=sys.stderr,
            )

    # Compute statistics using median and MAD for robustness against outliers
    median_ns = statistics.median(sample_ns_values)
    if len(sample_ns_values) > 1:
        # MAD = median(|x_i - median(x)|), scaled to be comparable to stddev
        deviations = [abs(x - median_ns) for x in sample_ns_values]
        mad_ns = statistics.median(deviations) * MAD_NORMAL_SCALE_FACTOR
    else:
        mad_ns = 0.0
    min_ns = min(sample_ns_values)
    max_ns = max(sample_ns_values)

    return BenchmarkResult(
        case_name=case_name,
        iterations_per_sample=iterations_per_sample,
        sample_count=actual_samples,
        per_iteration_ns=median_ns,
        per_iteration_ns_mad=mad_ns,
        per_iteration_ns_min=min_ns,
        per_iteration_ns_max=max_ns,
        shell_path=shell_path,
    )


# =============================================================================
# Output formatting
# =============================================================================


def format_duration_ns(ns: float) -> str:
    """Format a duration in nanoseconds to a human-readable string."""
    if ns >= NS_PER_SECOND:
        return f"{ns / NS_PER_SECOND:.3f} s"
    elif ns >= 1e6:
        return f"{ns / 1e6:.3f} ms"
    elif ns >= 1e3:
        return f"{ns / 1e3:.3f} µs"
    else:
        return f"{ns:.3f} ns"


def format_result_with_variance(result: BenchmarkResult) -> str:
    """
    Format a benchmark result with variance indicator.

    Uses MAD-based coefficient of variation for robustness against outliers.
    """
    median_str = format_duration_ns(result.per_iteration_ns)

    if result.sample_count <= 1 or result.per_iteration_ns == 0:
        return f"{median_str}/iter"

    # Coefficient of variation using MAD (scaled to be stddev-comparable)
    cv_pct = (result.per_iteration_ns_mad / result.per_iteration_ns) * 100

    # Flag high variance with warning indicator
    if cv_pct > HIGH_VARIANCE_THRESHOLD_PCT:
        return f"{median_str}/iter (±{cv_pct:.1f}% ⚠️)"
    else:
        return f"{median_str}/iter (±{cv_pct:.1f}%)"


def format_change(change: float) -> str:
    """
    Format performance change ratio with emoji indicator and percentage.

    Args:
        change: Ratio of reference_time / test_time. Values > 1.0 mean
                the test shell is faster than the reference.

    Returns:
        Formatted string with emoji and percentage change.
    """
    # Calculate percentage change (negative = faster, positive = slower)
    # change > 1 means test is faster, so pct_change should be negative
    pct_change = (1.0 / change - 1.0) * 100

    if change >= CHANGE_FASTER_THRESHOLD:
        emoji = "🚀"
        text = f"{pct_change:.1f}% (faster)"
    elif change >= CHANGE_SIMILAR_THRESHOLD:
        emoji = "⚖️ "
        text = f"{pct_change:+.1f}% (similar)"
    else:
        emoji = "🐢"
        text = f"+{pct_change:.1f}% (slower)"
    return f"{emoji} {text}"


def print_human_single_shell_results(results: list[BenchmarkResult], shell: str):
    """Print results for a single shell (no comparison)."""
    print()
    print("=" * 70)
    print("📊 Shell Benchmark Results")
    print("=" * 70)
    print(f"  Shell: {shell}")
    print("=" * 70)
    print()

    for result in results:
        print(f"🧪 {result.case_name}")
        print(f"   {format_result_with_variance(result)}")
        print(f"   ({result.sample_count} samples, {result.iterations_per_sample} iters/sample)")
        print()


def compute_geometric_mean_change(comparisons: list[ComparisonResult]) -> float:
    """
    Compute the geometric mean of change ratios across comparisons.

    Geometric mean is the correct way to average ratios/rates. Returns 1.0
    if the list is empty or if any change ratio is non-positive (which would
    indicate a measurement error).
    """
    if not comparisons:
        return 1.0

    # Filter out any non-positive values (shouldn't happen, but guard against it)
    valid_changes = [comp.change for comp in comparisons if comp.change > 0]
    if not valid_changes:
        return 1.0

    log_sum = sum(math.log(change) for change in valid_changes)
    return math.exp(log_sum / len(valid_changes))


def print_human_results(
    comparisons: list[ComparisonResult], test_shell: str, ref_shell: str
):
    """Print comparison results in human-readable format with emoji."""
    print()
    print("=" * 70)
    print("📊 Shell Benchmark Results")
    print("=" * 70)
    print(f"  Test shell:      {test_shell}")
    print(f"  Reference shell: {ref_shell}")
    print("=" * 70)
    print()

    for comp in comparisons:
        print(f"🧪 {comp.case_name}")
        print(f"   Test:      {format_result_with_variance(comp.test_result)}")
        print(f"   Reference: {format_result_with_variance(comp.reference_result)}")
        print(f"   Change:    {format_change(comp.change)}")
        print()

    # Summary - use geometric mean for averaging ratios (more statistically sound)
    print("-" * 70)
    avg_change = compute_geometric_mean_change(comparisons)
    print(f"📈 Overall change (geometric mean): {format_change(avg_change)}")
    print()


def _result_to_json(result: BenchmarkResult) -> dict:
    """
    Convert a BenchmarkResult to a JSON-serializable dict.

    Note: per_iteration_ns is the median, and per_iteration_ns_mad is the
    MAD (median absolute deviation) scaled by 1.4826 for normal comparability.
    """
    return {
        "iterations_per_sample": result.iterations_per_sample,
        "sample_count": result.sample_count,
        "per_iteration_ns": result.per_iteration_ns,
        "per_iteration_ns_mad": result.per_iteration_ns_mad,
        "per_iteration_ns_min": result.per_iteration_ns_min,
        "per_iteration_ns_max": result.per_iteration_ns_max,
    }


def print_json_single_shell_results(results: list[BenchmarkResult], shell: str):
    """Print results for a single shell in JSON format."""
    output = {
        "shell": shell,
        "results": [
            {
                "name": result.case_name,
                **_result_to_json(result),
            }
            for result in results
        ],
    }
    print(json.dumps(output, indent=2))


def print_json_results(
    comparisons: list[ComparisonResult], test_shell: str, ref_shell: str
):
    """Print comparison results in JSON format."""
    avg_change = compute_geometric_mean_change(comparisons)
    output = {
        "test_shell": test_shell,
        "reference_shell": ref_shell,
        "results": [
            {
                "name": comp.case_name,
                "test": _result_to_json(comp.test_result),
                "reference": _result_to_json(comp.reference_result),
                "change": comp.change,
            }
            for comp in comparisons
        ],
        "overall_change": avg_change,
    }
    print(json.dumps(output, indent=2))


# =============================================================================
# Main entry point
# =============================================================================


def resolve_shell_path(shell: str) -> str:
    """
    Resolve shell path, checking it exists and is executable.

    Args:
        shell: Shell name or path (e.g., 'bash' or '/usr/bin/bash')

    Returns:
        Resolved absolute path to the shell executable.

    Exits with error if shell is not found or not executable.
    """
    # If it's an absolute or relative path, use as-is
    if "/" in shell:
        resolved = shell
    else:
        # Search PATH
        resolved = shutil.which(shell)
        if resolved is None:
            print(f"Error: Shell '{shell}' not found in PATH", file=sys.stderr)
            sys.exit(1)

    # Verify it exists and is executable
    if not os.path.isfile(resolved):
        print(f"Error: Shell path '{resolved}' does not exist or is not a file", file=sys.stderr)
        sys.exit(1)
    if not os.access(resolved, os.X_OK):
        print(f"Error: Shell '{resolved}' is not executable", file=sys.stderr)
        sys.exit(1)

    return resolved


def _positive_float(value: str) -> float:
    """Argparse type validator for positive float values."""
    try:
        fval = float(value)
    except ValueError:
        raise argparse.ArgumentTypeError(f"invalid float value: '{value}'")
    if fval < 0:
        raise argparse.ArgumentTypeError(f"value must be non-negative: {fval}")
    return fval


def _positive_int(value: str) -> int:
    """Argparse type validator for positive integer values."""
    try:
        ival = int(value)
    except ValueError:
        raise argparse.ArgumentTypeError(f"invalid integer value: '{value}'")
    if ival <= 0:
        raise argparse.ArgumentTypeError(f"value must be positive: {ival}")
    return ival


def main():
    parser = argparse.ArgumentParser(
        description="Shell benchmarking harness for brush",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Available benchmarks:
  assignment    - Variable assignment (x=42)
  colon         - No-op builtin (:)
  echo_builtin  - Echo builtin to /dev/null
  echo_cmd      - External /usr/bin/echo to /dev/null
  increment     - Arithmetic increment ((x++))
  subshell      - Subshell fork ((:))
  cmdsubst      - Command substitution ($(...))
  var_expand    - Variable expansion ("$var")
  func_call     - Function invocation
  array_access  - Array element access (${arr[i]})
  pattern_match - Glob pattern match in [[ ]]
  regex_match   - Regex match in [[ =~ ]]
  if_taken      - If conditional (branch taken)
  if_not_taken  - If conditional (branch not taken)

Examples:
  %(prog)s --shell ./target/release/brush
  %(prog)s --shell brush --reference-shell bash --benchmarks colon increment
  %(prog)s --shell brush --duration 5 --json
""",
    )

    parser.add_argument(
        "-s",
        "--shell",
        dest="shell",
        type=str,
        required=True,
        help="Path to test shell binary",
    )

    parser.add_argument(
        "-r",
        "--reference-shell",
        dest="reference_shell",
        type=str,
        default=None,
        help="Path to reference shell for comparison (optional; if omitted, only test shell is benchmarked)",
    )

    parser.add_argument(
        "-b",
        "--benchmarks",
        dest="benchmarks",
        type=str,
        nargs="+",
        default=list(BENCHMARK_CASES.keys()),
        choices=list(BENCHMARK_CASES.keys()),
        help="Name(s) of benchmark(s) to run (default: all)",
    )

    parser.add_argument(
        "--samples",
        dest="samples",
        type=_positive_int,
        default=DEFAULT_SAMPLES,
        help=f"Number of samples to collect per benchmark (default: {DEFAULT_SAMPLES})",
    )

    parser.add_argument(
        "-d",
        "--duration",
        dest="duration",
        type=_positive_float,
        default=DEFAULT_DURATION_SECONDS,
        help=f"Number of seconds to run each test (default: {DEFAULT_DURATION_SECONDS})",
    )

    parser.add_argument(
        "-w",
        "--warmup-duration",
        dest="warmup_duration",
        type=_positive_float,
        default=DEFAULT_WARMUP_DURATION_SECONDS,
        help=f"Warmup duration in seconds; 0 to skip (default: {DEFAULT_WARMUP_DURATION_SECONDS})",
    )

    parser.add_argument(
        "-j",
        "--json",
        dest="json_output",
        action="store_true",
        help="Output results in JSON format",
    )

    parser.add_argument(
        "-v",
        "--verbose",
        dest="verbose",
        action="store_true",
        help="Print progress information",
    )

    args = parser.parse_args()

    # Resolve shell paths
    test_shell = resolve_shell_path(args.shell)
    ref_shell = resolve_shell_path(args.reference_shell) if args.reference_shell else None

    if not args.json_output:
        print("🐚 Shell Benchmark Harness", file=sys.stderr)
        print(f"   Test shell:      {test_shell}", file=sys.stderr)
        if ref_shell:
            print(f"   Reference shell: {ref_shell}", file=sys.stderr)
        print(f"   Duration:        {args.duration}s per benchmark", file=sys.stderr)
        print(f"   Warmup:          {args.warmup_duration}s", file=sys.stderr)
        print(f"   Samples:         {args.samples}", file=sys.stderr)
        print(f"   Benchmarks:      {', '.join(args.benchmarks)}", file=sys.stderr)
        print(file=sys.stderr)

    # Single-shell mode (no reference)
    if ref_shell is None:
        results = []
        for bench_name in args.benchmarks:
            case = BENCHMARK_CASES[bench_name]

            if not args.json_output:
                print(f"▶️  Running benchmark: {bench_name}", file=sys.stderr)
                print(f"   Testing: {test_shell}", file=sys.stderr)

            result = run_benchmark(
                test_shell,
                bench_name,
                case,
                args.duration,
                args.samples,
                args.warmup_duration,
                verbose=args.verbose,
            )
            results.append(result)

            if not args.json_output:
                print(file=sys.stderr)

        if args.json_output:
            print_json_single_shell_results(results, test_shell)
        else:
            print_human_single_shell_results(results, test_shell)
        return

    # Comparison mode (test vs reference)
    comparisons = []

    for bench_name in args.benchmarks:
        case = BENCHMARK_CASES[bench_name]

        if not args.json_output:
            print(f"▶️  Running benchmark: {bench_name}", file=sys.stderr)

        # Run on test shell
        if not args.json_output:
            print(f"   Testing: {test_shell}", file=sys.stderr)
        test_result = run_benchmark(
            test_shell,
            bench_name,
            case,
            args.duration,
            args.samples,
            args.warmup_duration,
            verbose=args.verbose,
        )

        # Run on reference shell
        if not args.json_output:
            print(f"   Testing: {ref_shell}", file=sys.stderr)
        ref_result = run_benchmark(
            ref_shell,
            bench_name,
            case,
            args.duration,
            args.samples,
            args.warmup_duration,
            verbose=args.verbose,
        )

        # Calculate performance change (reference time / test time)
        # > 1 means test shell is faster
        # Note: per_iteration_ns is the median value
        if test_result.per_iteration_ns > 0:
            change = ref_result.per_iteration_ns / test_result.per_iteration_ns
        else:
            # Test result was unmeasurably fast; treat as equivalent
            change = 1.0

        comparisons.append(
            ComparisonResult(
                case_name=bench_name,
                test_result=test_result,
                reference_result=ref_result,
                change=change,
            )
        )

        if not args.json_output:
            print(file=sys.stderr)

    # Output results
    if args.json_output:
        print_json_results(comparisons, test_shell, ref_shell)
    else:
        print_human_results(comparisons, test_shell, ref_shell)


if __name__ == "__main__":
    main()
