#!/usr/bin/env python3
"""
Shell benchmarking harness for brush.

Runs shell script snippets against test and reference shells using
warmup, calibration, and timed execution phases.
"""

import argparse
import json
import math
import re
import shutil
import statistics
import subprocess
import sys
from dataclasses import dataclass, field
from typing import Optional


@dataclass
class BenchmarkCase:
    """
    A benchmark case measuring a specific shell operation.

    The benchmark runs `loop_body` in a tight loop, with optional `setup`
    executed once before timing and `cleanup` after.
    """

    name: str
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

    Contains per-iteration timing statistics (mean, stddev, min, max) computed
    from `sample_count` independent timed runs.
    """

    case_name: str
    iterations_per_sample: int
    sample_count: int
    per_iteration_ns: float  # mean across samples
    per_iteration_ns_stddev: float
    per_iteration_ns_min: float
    per_iteration_ns_max: float
    shell_path: str


@dataclass
class ComparisonResult:
    """
    Performance comparison between test shell and reference shell.

    The `speedup` ratio is reference_time / test_time, so values > 1.0
    indicate the test shell is faster than the reference.
    """

    case_name: str
    test_result: BenchmarkResult
    reference_result: BenchmarkResult
    speedup: float


# =============================================================================
# Built-in benchmark cases
# =============================================================================

BENCHMARK_CASES: dict[str, BenchmarkCase] = {
    "assignment": BenchmarkCase(
        name="assignment",
        loop_body="x=42",
    ),
    "colon": BenchmarkCase(
        name="colon",
        loop_body=":",
    ),
    "echo_builtin": BenchmarkCase(
        name="echo_builtin",
        loop_body="echo >/dev/null",
    ),
    "echo_cmd": BenchmarkCase(
        name="echo_cmd",
        loop_body="/usr/bin/echo >/dev/null",
    ),
    "increment": BenchmarkCase(
        name="increment",
        setup="x=0",
        loop_body="((x++))",
    ),
    "subshell": BenchmarkCase(
        name="subshell",
        loop_body="(:)",
    ),
    "cmdsubst": BenchmarkCase(
        name="cmdsubst",
        loop_body=': $(:)',
    ),
    "var_expand": BenchmarkCase(
        name="var_expand",
        setup='myvar="hello world"',
        loop_body=': "$myvar"',
    ),
    "func_call": BenchmarkCase(
        name="func_call",
        setup="myfunc() { :; }",
        loop_body="myfunc",
    ),
    "array_access": BenchmarkCase(
        name="array_access",
        setup='myarr=(a b c d e f g h i j)',
        loop_body=': "${myarr[5]}"',
    ),
    "pattern_match": BenchmarkCase(
        name="pattern_match",
        loop_body='[[ "hello world" == hello* ]]',
    ),
    "regex_match": BenchmarkCase(
        name="regex_match",
        loop_body='[[ "hello world" =~ ^hello.* ]]',
    ),
    "if_taken": BenchmarkCase(
        name="if_taken",
        loop_body="if [[ 1 -eq 1 ]]; then :; fi",
    ),
    "if_not_taken": BenchmarkCase(
        name="if_not_taken",
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

    WARNING: This parser is fragile and shell-dependent. It relies on bash's
    `time` keyword output format (not /usr/bin/time). Other shells (zsh, dash,
    ksh) or non-default TIMEFORMAT settings may produce incompatible output.
    Locale settings (LC_NUMERIC) could also affect decimal separators.
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
    """Generate a complete benchmark script for the given case."""
    lines = []

    # Setup (optional)
    if case.setup:
        lines.append(case.setup)

    # Timed loop
    lines.append(f"time {{ for ((i=0; i<{iterations}; i++)); do {case.loop_body}; done; }}")

    # Cleanup (optional)
    if case.cleanup:
        lines.append(case.cleanup)

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

# Constants for benchmark phases
WARMUP_ITERATIONS = 10
CALIBRATION_MIN_ITERATIONS = 100
CALIBRATION_MIN_TIME = 0.1  # Minimum seconds for reliable calibration
CALIBRATION_MAX_ITERATIONS = 100_000_000  # Safety cap
MAX_TARGET_ITERATIONS = 1_000_000_000  # Prevent runaway iteration counts

# Constants for statistical sampling
DEFAULT_SAMPLES = 5
MIN_SAMPLE_DURATION = 0.5  # Minimum seconds per sample for reliable measurement


def run_benchmark_phase(
    shell_path: str, case: BenchmarkCase, iterations: int
) -> TimingResult:
    """Run a single benchmark phase and return timing."""
    script = generate_benchmark_script(case, iterations)
    result = run_shell_script(shell_path, script)

    if result.returncode != 0:
        raise RuntimeError(
            f"Shell execution failed (exit {result.returncode}):\n"
            f"stdout: {result.stdout}\n"
            f"stderr: {result.stderr}"
        )

    return parse_time_output(result.stderr)


def run_benchmark(
    shell_path: str,
    case: BenchmarkCase,
    target_duration: float,
    num_samples: int = DEFAULT_SAMPLES,
    verbose: bool = False,
) -> BenchmarkResult:
    """
    Run a complete benchmark with warmup, calibration, and multiple sample phases.

    Args:
        shell_path: Path to shell executable
        case: Benchmark case to run
        target_duration: Total time budget in seconds (divided among samples)
        num_samples: Number of independent samples to collect
        verbose: Print progress information

    Returns:
        BenchmarkResult with timing statistics across all samples
    """
    # Determine actual sample count based on duration constraints
    sample_duration = target_duration / num_samples
    actual_samples = num_samples

    if sample_duration < MIN_SAMPLE_DURATION:
        actual_samples = max(1, int(target_duration / MIN_SAMPLE_DURATION))
        sample_duration = target_duration / actual_samples
        # Always warn when we reduce samples (even in non-verbose mode)
        print(
            f"  ⚠️  Duration too short for {num_samples} samples; "
            f"using {actual_samples} sample(s) of {sample_duration:.1f}s instead",
            file=sys.stderr,
        )

    if verbose:
        print("  🔥 Warming up...", file=sys.stderr)

    # Phase 1: Warmup
    run_benchmark_phase(shell_path, case, WARMUP_ITERATIONS)

    if verbose:
        print("  📏 Calibrating...", file=sys.stderr)

    # Phase 2: Adaptive calibration
    # Keep doubling iterations until we get a measurable time
    calibration_iterations = CALIBRATION_MIN_ITERATIONS
    calibration_timing: Optional[TimingResult] = None
    while calibration_iterations <= CALIBRATION_MAX_ITERATIONS:
        calibration_timing = run_benchmark_phase(shell_path, case, calibration_iterations)

        if calibration_timing.real_seconds >= CALIBRATION_MIN_TIME:
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
    if calibration_timing.real_seconds < CALIBRATION_MIN_TIME:
        # Even at max iterations, still too fast - use what we have
        if verbose:
            print(
                f"     (warning: calibration capped at {calibration_iterations} iterations)",
                file=sys.stderr,
            )

    # Calculate iterations needed for each sample
    per_iteration_seconds = calibration_timing.real_seconds / calibration_iterations
    if per_iteration_seconds <= 0:
        # Fallback if calibration was too fast
        per_iteration_seconds = 1e-9

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
        per_iter_ns = (sample_timing.real_seconds / iterations_per_sample) * 1e9
        sample_ns_values.append(per_iter_ns)

        if verbose:
            print(
                f"     Sample {sample_idx + 1}/{actual_samples}: "
                f"{format_duration_ns(per_iter_ns)}/iter",
                file=sys.stderr,
            )

    # Compute statistics
    mean_ns = statistics.mean(sample_ns_values)
    stddev_ns = statistics.stdev(sample_ns_values) if len(sample_ns_values) > 1 else 0.0
    min_ns = min(sample_ns_values)
    max_ns = max(sample_ns_values)

    return BenchmarkResult(
        case_name=case.name,
        iterations_per_sample=iterations_per_sample,
        sample_count=actual_samples,
        per_iteration_ns=mean_ns,
        per_iteration_ns_stddev=stddev_ns,
        per_iteration_ns_min=min_ns,
        per_iteration_ns_max=max_ns,
        shell_path=shell_path,
    )


# =============================================================================
# Output formatting
# =============================================================================


def format_duration_ns(ns: float) -> str:
    """Format a duration in nanoseconds to a human-readable string."""
    if ns >= 1e9:
        return f"{ns / 1e9:.3f} s"
    elif ns >= 1e6:
        return f"{ns / 1e6:.3f} ms"
    elif ns >= 1e3:
        return f"{ns / 1e3:.3f} µs"
    else:
        return f"{ns:.3f} ns"


def format_result_with_variance(result: BenchmarkResult) -> str:
    """Format a benchmark result with variance indicator."""
    mean_str = format_duration_ns(result.per_iteration_ns)

    if result.sample_count <= 1 or result.per_iteration_ns == 0:
        return f"{mean_str}/iter"

    # Coefficient of variation (relative standard deviation)
    cv_pct = (result.per_iteration_ns_stddev / result.per_iteration_ns) * 100

    # Flag high variance with warning indicator
    if cv_pct > 10:
        return f"{mean_str}/iter (±{cv_pct:.1f}% ⚠️)"
    else:
        return f"{mean_str}/iter (±{cv_pct:.1f}%)"


def format_speedup(speedup: float) -> str:
    """Format speedup with emoji indicator and percentage change."""
    # Calculate percentage change (negative = faster, positive = slower)
    # speedup > 1 means test is faster, so pct_change should be negative
    pct_change = (1.0 / speedup - 1.0) * 100

    if speedup >= 1.1:
        emoji = "🚀"
        text = f"{pct_change:.1f}% (faster)"
    elif speedup >= 0.95:
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
        print(f"   Result:    {format_speedup(comp.speedup)}")
        print()

    # Summary
    print("-" * 70)
    avg_speedup = (
        sum(c.speedup for c in comparisons) / len(comparisons) if comparisons else 1.0
    )
    print(f"📈 Average speedup: {format_speedup(avg_speedup)}")
    print()


def _result_to_json(result: BenchmarkResult) -> dict:
    """Convert a BenchmarkResult to a JSON-serializable dict."""
    return {
        "iterations_per_sample": result.iterations_per_sample,
        "sample_count": result.sample_count,
        "per_iteration_ns": result.per_iteration_ns,
        "per_iteration_ns_stddev": result.per_iteration_ns_stddev,
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
    output = {
        "test_shell": test_shell,
        "reference_shell": ref_shell,
        "results": [
            {
                "name": comp.case_name,
                "test": _result_to_json(comp.test_result),
                "reference": _result_to_json(comp.reference_result),
                "speedup": comp.speedup,
            }
            for comp in comparisons
        ],
    }
    print(json.dumps(output, indent=2))


# =============================================================================
# Main entry point
# =============================================================================


def resolve_shell_path(shell: str) -> str:
    """Resolve shell path, checking it exists."""
    # If it's an absolute or relative path, use as-is
    if "/" in shell:
        return shell

    # Otherwise, search PATH
    resolved = shutil.which(shell)
    if resolved is None:
        print(f"Error: Shell '{shell}' not found in PATH", file=sys.stderr)
        sys.exit(1)
    return resolved


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
        type=int,
        default=DEFAULT_SAMPLES,
        help=f"Number of samples to collect per benchmark (default: {DEFAULT_SAMPLES})",
    )

    parser.add_argument(
        "-d",
        "--duration",
        dest="duration",
        type=float,
        default=10.0,
        help="Number of seconds to run each test (default: 10)",
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
                test_shell, case, args.duration, args.samples, verbose=args.verbose
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
            test_shell, case, args.duration, args.samples, verbose=args.verbose
        )

        # Run on reference shell
        if not args.json_output:
            print(f"   Testing: {ref_shell}", file=sys.stderr)
        ref_result = run_benchmark(
            ref_shell, case, args.duration, args.samples, verbose=args.verbose
        )

        # Calculate speedup (reference time / test time)
        # > 1 means test shell is faster
        speedup = ref_result.per_iteration_ns / test_result.per_iteration_ns

        comparisons.append(
            ComparisonResult(
                case_name=case.name,
                test_result=test_result,
                reference_result=ref_result,
                speedup=speedup,
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
