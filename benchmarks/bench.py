#!/usr/bin/env python3
"""
Shell benchmarking harness for brush.

Runs shell script snippets against test and reference shells using
warmup, calibration, and timed execution phases.
"""

import argparse
import json
import re
import shutil
import subprocess
import sys
from dataclasses import dataclass, field
from typing import Optional


@dataclass
class BenchmarkCase:
    """Definition of a benchmark test case."""

    name: str
    loop_body: str
    setup: Optional[str] = None
    cleanup: Optional[str] = None


@dataclass
class TimingResult:
    """Parsed timing result from shell's time builtin."""

    real_seconds: float
    user_seconds: float
    sys_seconds: float


@dataclass
class BenchmarkResult:
    """Result of running a benchmark."""

    case_name: str
    iterations: int
    total_time: TimingResult
    per_iteration_ns: float
    shell_path: str


@dataclass
class ComparisonResult:
    """Comparison between test and reference shell results."""

    case_name: str
    test_result: BenchmarkResult
    reference_result: BenchmarkResult
    speedup: float  # > 1 means test is faster


# =============================================================================
# Built-in benchmark cases
# =============================================================================

BENCHMARK_CASES: dict[str, BenchmarkCase] = {
    "colon": BenchmarkCase(
        name="colon",
        loop_body=":",
    ),
    "increment": BenchmarkCase(
        name="increment",
        setup="x=0",
        loop_body="((x++))",
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
    verbose: bool = False,
) -> BenchmarkResult:
    """
    Run a complete benchmark with warmup, calibration, and main phases.

    Args:
        shell_path: Path to shell executable
        case: Benchmark case to run
        target_duration: Target duration in seconds for main run
        verbose: Print progress information

    Returns:
        BenchmarkResult with timing data
    """
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

    # Calculate iterations needed for target duration
    per_iteration_seconds = calibration_timing.real_seconds / calibration_iterations
    if per_iteration_seconds <= 0:
        # Fallback if calibration was too fast
        per_iteration_seconds = 1e-9

    target_iterations = max(1, int(target_duration / per_iteration_seconds))

    if verbose:
        print(
            f"  ⏱️  Running {target_iterations} iterations (est. {target_duration:.1f}s)...",
            file=sys.stderr,
        )

    # Phase 3: Main run
    main_timing = run_benchmark_phase(shell_path, case, target_iterations)

    # Calculate per-iteration time in nanoseconds
    per_iteration_ns = (main_timing.real_seconds / target_iterations) * 1e9

    return BenchmarkResult(
        case_name=case.name,
        iterations=target_iterations,
        total_time=main_timing,
        per_iteration_ns=per_iteration_ns,
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


def print_human_results(comparisons: list[ComparisonResult], test_shell: str, ref_shell: str):
    """Print results in human-readable format with emoji."""
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
        print(f"   Test:      {format_duration_ns(comp.test_result.per_iteration_ns)}/iter")
        print(f"   Reference: {format_duration_ns(comp.reference_result.per_iteration_ns)}/iter")
        print(f"   Result:    {format_speedup(comp.speedup)}")
        print()

    # Summary
    print("-" * 70)
    avg_speedup = sum(c.speedup for c in comparisons) / len(comparisons) if comparisons else 1.0
    print(f"📈 Average speedup: {format_speedup(avg_speedup)}")
    print()


def print_json_results(comparisons: list[ComparisonResult], test_shell: str, ref_shell: str):
    """Print results in JSON format."""
    output = {
        "test_shell": test_shell,
        "reference_shell": ref_shell,
        "results": [
            {
                "name": comp.case_name,
                "test": {
                    "iterations": comp.test_result.iterations,
                    "total_real_seconds": comp.test_result.total_time.real_seconds,
                    "total_user_seconds": comp.test_result.total_time.user_seconds,
                    "total_sys_seconds": comp.test_result.total_time.sys_seconds,
                    "per_iteration_ns": comp.test_result.per_iteration_ns,
                },
                "reference": {
                    "iterations": comp.reference_result.iterations,
                    "total_real_seconds": comp.reference_result.total_time.real_seconds,
                    "total_user_seconds": comp.reference_result.total_time.user_seconds,
                    "total_sys_seconds": comp.reference_result.total_time.sys_seconds,
                    "per_iteration_ns": comp.reference_result.per_iteration_ns,
                },
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
Available benchmark tests:
  colon      - Invoke the ':' builtin with no args
  increment  - Increment an integer variable

Examples:
  %(prog)s --shell ./target/release/brush
  %(prog)s --shell brush --reference-shell bash --tests colon increment
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
        default="bash",
        help="Path to reference shell binary (default: bash)",
    )

    parser.add_argument(
        "-t",
        "--tests",
        dest="tests",
        type=str,
        nargs="+",
        default=list(BENCHMARK_CASES.keys()),
        choices=list(BENCHMARK_CASES.keys()),
        help="Name(s) of test(s) to run (default: all)",
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
    ref_shell = resolve_shell_path(args.reference_shell)

    if not args.json_output:
        print("🐚 Shell Benchmark Harness", file=sys.stderr)
        print(f"   Test shell:      {test_shell}", file=sys.stderr)
        print(f"   Reference shell: {ref_shell}", file=sys.stderr)
        print(f"   Duration:        {args.duration}s per test", file=sys.stderr)
        print(f"   Tests:           {', '.join(args.tests)}", file=sys.stderr)
        print(file=sys.stderr)

    comparisons = []

    for test_name in args.tests:
        case = BENCHMARK_CASES[test_name]

        if not args.json_output:
            print(f"▶️  Running benchmark: {test_name}", file=sys.stderr)

        # Run on test shell
        if not args.json_output:
            print(f"   Testing: {test_shell}", file=sys.stderr)
        test_result = run_benchmark(
            test_shell, case, args.duration, verbose=args.verbose
        )

        # Run on reference shell
        if not args.json_output:
            print(f"   Testing: {ref_shell}", file=sys.stderr)
        ref_result = run_benchmark(
            ref_shell, case, args.duration, verbose=args.verbose
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
