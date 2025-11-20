#!/usr/bin/env python3
"""
Enhanced bash compatibility test runner for brush.

Runs bash test suites with timeout protection, pass/fail tracking, and reporting.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ProcessPoolExecutor, as_completed
from dataclasses import dataclass, asdict
from typing import List, Dict, Optional
from pathlib import Path
from multiprocessing import Manager, cpu_count


# ANSI color codes
class Colors:
    """ANSI color codes for terminal output."""
    RESET = '\033[0m'
    BOLD = '\033[1m'
    DIM = '\033[2m'
    
    # Status colors
    GREEN = '\033[32m'
    RED = '\033[31m'
    YELLOW = '\033[33m'
    BLUE = '\033[34m'
    MAGENTA = '\033[35m'
    CYAN = '\033[36m'
    
    # Bright variants
    BRIGHT_GREEN = '\033[92m'
    BRIGHT_RED = '\033[91m'
    BRIGHT_YELLOW = '\033[93m'
    BRIGHT_BLUE = '\033[94m'
    BRIGHT_CYAN = '\033[96m'
    
    @staticmethod
    def enabled():
        """Check if colors should be enabled."""
        return sys.stderr.isatty() and os.getenv('NO_COLOR') is None


def colorize(text: str, color: str) -> str:
    """Colorize text if colors are enabled."""
    if Colors.enabled():
        return f"{color}{text}{Colors.RESET}"
    return text


def bold(text: str) -> str:
    """Make text bold if colors are enabled."""
    if Colors.enabled():
        return f"{Colors.BOLD}{text}{Colors.RESET}"
    return text


def progress_bar(current: int, total: int, width: int = 30) -> str:
    """Generate a visual progress bar."""
    if not Colors.enabled():
        # Simple text-based progress without colors
        return f"{current}/{total}"
    
    fraction = current / total if total > 0 else 0
    filled = int(width * fraction)
    empty = width - filled
    
    # Use block characters for a nice progress bar
    bar = "█" * filled + "░" * empty
    percentage = int(fraction * 100)
    
    # Color the bar based on progress
    if percentage == 100:
        bar_colored = colorize(bar, Colors.BRIGHT_GREEN)
    elif percentage >= 50:
        bar_colored = colorize(bar, Colors.CYAN)
    else:
        bar_colored = colorize(bar, Colors.YELLOW)
    
    return f"{bar_colored} {percentage}%"


@dataclass
class TestResult:
    """Result of running a single test."""
    name: str
    status: str  # "pass", "fail", "timeout", "error"
    duration: float
    output: str = ""
    error: str = ""


@dataclass
class SuiteResult:
    """Result of running a test suite."""
    suite_name: str
    total: int
    passed: int
    failed: int
    timeout: int
    error: int
    duration: float
    tests: List[TestResult]


class BashTestRunner:
    def __init__(self, bash_source_dir: str, shell_path: str, timeout: int = 30, verbose: bool = False):
        self.bash_source_dir = Path(bash_source_dir).resolve()
        self.shell_path = Path(shell_path).resolve()
        self.timeout = timeout
        self.verbose = verbose
        self.tests_dir = self.bash_source_dir / "tests"
        
        if not self.shell_path.exists():
            raise ValueError(f"Shell not found: {self.shell_path}")
        if not self.tests_dir.exists():
            raise ValueError(f"Tests directory not found: {self.tests_dir}")
    
    def get_base_env(self) -> Dict[str, str]:
        """Get the base environment for running tests."""
        env = os.environ.copy()
        env["BUILD_DIR"] = str(self.bash_source_dir)
        env["THIS_SH"] = str(self.shell_path)
        env["PATH"] = f"{self.tests_dir}:{env['PATH']}"
        return env
    
    def list_tests(self) -> List[str]:
        """List all available test cases."""
        tests = set()
        for file in self.tests_dir.glob("*.tests"):
            tests.add(file.stem)
        return sorted(tests)
    
    def get_suite_tests(self, suite_name: str) -> List[str]:
        """Get the list of tests in a suite by parsing run-<suite> script."""
        suite_script = self.tests_dir / f"run-{suite_name}"
        if not suite_script.exists():
            raise ValueError(f"Suite not found: {suite_name}")
        
        # Parse the suite script to find test names
        tests = []
        with open(suite_script) as f:
            content = f.read()
            
            # Check if this is a composite suite (runs other run-* scripts)
            import re
            
            # Look for "sh $x" or "sh run-*" patterns which indicate running sub-tests
            # Match run- followed by alphanumeric and hyphens
            run_matches = list(re.finditer(r'(?:sh\s+run-([\w-]+)|run-([\w-]+)[|)])', content))
            
            if run_matches:
                # This is a composite suite like minimal or all
                seen = set()
                for match in run_matches:
                    test_name = match.group(1) or match.group(2)
                    # Avoid recursive references and duplicates
                    if test_name and test_name != suite_name and test_name not in ['all', 'minimal'] and test_name not in seen:
                        tests.append(test_name)
                        seen.add(test_name)
            else:
                # This is a single test runner - just run the test with same name
                tests = [suite_name]
        
        return tests
    
    def run_single_test(self, test_name: str) -> TestResult:
        """Run a single test and return the result."""
        start_time = time.time()

        with tempfile.TemporaryDirectory() as temp_dir:
            test_output_path = Path(temp_dir) / "test.out"

            env = self.get_base_env()
            env["BASH_TSTOUT"] = str(test_output_path)

            # Check if this is a run-* script or a .tests file
            run_script_path = self.tests_dir / f"run-{test_name}"
            tests_file_path = self.tests_dir / f"{test_name}.tests"

            # Handle naming inconsistency: run-minimal references "run-ifs-tests" but file is "run-ifs"
            if not run_script_path.exists() and test_name.endswith('-tests'):
                alt_name = test_name.rsplit('-tests', 1)[0]
                alt_run_script = self.tests_dir / f"run-{alt_name}"
                if alt_run_script.exists():
                    run_script_path = alt_run_script

            if run_script_path.exists():
                # Use run-* script with sh
                cmd = ["sh", str(run_script_path)]
                # run-* scripts do their own diff, so we check exit code
                use_exit_code = True
                expected_output_path = None
            elif tests_file_path.exists():
                # Use .tests file with the shell being tested
                cmd = [str(self.shell_path), str(tests_file_path)]
                expected_output_path = self.tests_dir / f"{test_name}.right"
                use_exit_code = False
            else:
                # Neither exists - error
                duration = time.time() - start_time
                return TestResult(
                    name=test_name,
                    status="error",
                    duration=duration,
                    error=f"Neither run-{test_name} nor {test_name}.tests found"
                )
            
            try:
                result = subprocess.run(
                    cmd,
                    cwd=str(self.tests_dir),
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=self.timeout
                )

                duration = time.time() - start_time

                # For run-* scripts, just check exit code (they do their own diff)
                if use_exit_code:
                    if result.returncode == 0:
                        return TestResult(
                            name=test_name,
                            status="pass",
                            duration=duration,
                            output=result.stdout + result.stderr
                        )
                    else:
                        return TestResult(
                            name=test_name,
                            status="fail",
                            duration=duration,
                            output=result.stdout,
                            error=result.stderr
                        )

                # For .tests files, compare output with expected
                if expected_output_path and expected_output_path.exists():
                    # Read actual output
                    actual_output = ""
                    if test_output_path.exists():
                        actual_output = test_output_path.read_text()
                    else:
                        # Some tests output to stdout/stderr
                        actual_output = result.stdout + result.stderr

                    expected_output = expected_output_path.read_text()

                    if actual_output == expected_output:
                        return TestResult(
                            name=test_name,
                            status="pass",
                            duration=duration,
                            output=actual_output
                        )
                    else:
                        # Run diff to get details
                        diff_result = subprocess.run(
                            ["diff", "-u", str(expected_output_path), "-"],
                            input=actual_output,
                            capture_output=True,
                            text=True
                        )

                        return TestResult(
                            name=test_name,
                            status="fail",
                            duration=duration,
                            output=actual_output,
                            error=diff_result.stdout
                        )
                else:
                    # No expected output file - check exit code
                    if result.returncode == 0:
                        return TestResult(
                            name=test_name,
                            status="pass",
                            duration=duration,
                            output=result.stdout + result.stderr
                        )
                    else:
                        return TestResult(
                            name=test_name,
                            status="fail",
                            duration=duration,
                            output=result.stdout,
                            error=result.stderr
                        )
            
            except subprocess.TimeoutExpired:
                duration = time.time() - start_time
                return TestResult(
                    name=test_name,
                    status="timeout",
                    duration=duration,
                    error=f"Test exceeded timeout of {self.timeout}s"
                )
            
            except Exception as e:
                duration = time.time() - start_time
                return TestResult(
                    name=test_name,
                    status="error",
                    duration=duration,
                    error=str(e)
                )
    
    def run_suite(self, suite_name: str, test_filter: Optional[str] = None, jobs: int = 1) -> SuiteResult:
        """Run a test suite and return aggregated results.
        
        Args:
            suite_name: Name of the suite to run
            test_filter: Optional filter for test names
            jobs: Number of parallel jobs (1 = sequential, >1 = parallel)
        """
        if jobs > 1:
            return self._run_suite_parallel(suite_name, test_filter, jobs)
        else:
            return self._run_suite_sequential(suite_name, test_filter)
    
    def _run_suite_sequential(self, suite_name: str, test_filter: Optional[str] = None) -> SuiteResult:
        """Run tests sequentially with live progress updates."""
        start_time = time.time()
        
        if suite_name == "all":
            tests = self.list_tests()
        else:
            tests = self.get_suite_tests(suite_name)
        
        if test_filter:
            tests = [t for t in tests if test_filter in t]
        
        # Print initial suite header
        if not self.verbose:
            header = f"Running {len(tests)} test{'s' if len(tests) != 1 else ''}"
            print(colorize(header, Colors.BRIGHT_CYAN), file=sys.stderr, flush=True)
        
        results = []
        passed_count = 0
        failed_count = 0
        
        for i, test_name in enumerate(tests, 1):
            # Progress indicator (cargo nextest style)
            if not self.verbose:
                # Single line with progress bar and status
                pbar = progress_bar(i-1, len(tests), width=20)
                status_line = f"\r{pbar} "
                if results:
                    if passed_count > 0:
                        status_line += colorize(f'✓ {passed_count}', Colors.BRIGHT_GREEN) + " "
                    if failed_count > 0:
                        status_line += colorize(f'✗ {failed_count}', Colors.BRIGHT_RED) + " "
                status_line += colorize("Running", Colors.DIM) + f" {test_name:<25}"
                print(status_line, end='', file=sys.stderr, flush=True)
            else:
                # Verbose mode - more detailed
                header = colorize(f"[{i}/{len(tests)}]", Colors.BRIGHT_BLUE)
                print(f"\n{header} Running {test_name}...", file=sys.stderr)
            
            result = self.run_single_test(test_name)
            results.append(result)
            
            # Update counts
            if result.status == "pass":
                passed_count += 1
            elif result.status == "fail":
                failed_count += 1
            
            if self.verbose:
                status_symbol = {
                    "pass": colorize("✓", Colors.BRIGHT_GREEN),
                    "fail": colorize("✗", Colors.BRIGHT_RED),
                    "timeout": colorize("⏱", Colors.YELLOW),
                    "error": colorize("⚠", Colors.YELLOW)
                }.get(result.status, "?")
                duration_color = Colors.DIM if result.status == "pass" else Colors.RESET
                print(f"  {status_symbol} {result.status} {colorize(f'({result.duration:.2f}s)', duration_color)}", file=sys.stderr)
        
        # Clear progress line and show final summary
        if not self.verbose:
            print(f"\r{' ' * 100}\r", end='', file=sys.stderr)  # Clear line
        
        duration = time.time() - start_time
        
        passed = sum(1 for r in results if r.status == "pass")
        failed = sum(1 for r in results if r.status == "fail")
        timeout = sum(1 for r in results if r.status == "timeout")
        error = sum(1 for r in results if r.status == "error")
        
        # Print compact summary line after completion
        if not self.verbose:
            summary_parts = [f"Finished in {colorize(f'{duration:.2f}s', Colors.BRIGHT_CYAN)}"]
            
            if passed > 0:
                summary_parts.append(colorize(f'✓ {passed} passed', Colors.BRIGHT_GREEN))
            if failed > 0:
                summary_parts.append(colorize(f'✗ {failed} failed', Colors.BRIGHT_RED))
            if timeout > 0:
                summary_parts.append(colorize(f'⏱ {timeout} timeout', Colors.YELLOW))
            if error > 0:
                summary_parts.append(colorize(f'⚠ {error} error', Colors.YELLOW))
            
            print(": ".join(summary_parts), file=sys.stderr)
        
        return SuiteResult(
            suite_name=suite_name,
            total=len(results),
            passed=passed,
            failed=failed,
            timeout=timeout,
            error=error,
            duration=duration,
            tests=results
        )
    
    def _run_suite_parallel(self, suite_name: str, test_filter: Optional[str] = None, jobs: int = 4) -> SuiteResult:
        """Run tests in parallel with progress updates."""
        start_time = time.time()
        
        if suite_name == "all":
            tests = self.list_tests()
        else:
            tests = self.get_suite_tests(suite_name)
        
        if test_filter:
            tests = [t for t in tests if test_filter in t]
        
        # Print initial suite header
        if not self.verbose:
            header = f"Running {len(tests)} test{'s' if len(tests) != 1 else ''} with {jobs} worker{'s' if jobs != 1 else ''}"
            print(colorize(header, Colors.BRIGHT_CYAN), file=sys.stderr, flush=True)
        
        results = []
        completed_count = 0
        passed_count = 0
        failed_count = 0
        
        # Run tests in parallel
        with ProcessPoolExecutor(max_workers=jobs) as executor:
            # Submit all tests
            future_to_test = {
                executor.submit(self.run_single_test, test_name): test_name 
                for test_name in tests
            }
            
            # Process results as they complete
            for future in as_completed(future_to_test):
                test_name = future_to_test[future]
                try:
                    result = future.result()
                    results.append(result)
                    completed_count += 1
                    
                    # Update counts
                    if result.status == "pass":
                        passed_count += 1
                    elif result.status == "fail":
                        failed_count += 1
                    
                    # Update progress
                    if not self.verbose:
                        pbar = progress_bar(completed_count, len(tests), width=20)
                        status_line = f"\r{pbar} "
                        if passed_count > 0:
                            status_line += colorize(f'✓ {passed_count}', Colors.BRIGHT_GREEN) + " "
                        if failed_count > 0:
                            status_line += colorize(f'✗ {failed_count}', Colors.BRIGHT_RED) + " "
                        status_line += colorize("Completed", Colors.DIM) + f" {test_name:<25}"
                        print(status_line, end='', file=sys.stderr, flush=True)
                    else:
                        status_symbol = {
                            "pass": colorize("✓", Colors.BRIGHT_GREEN),
                            "fail": colorize("✗", Colors.BRIGHT_RED),
                            "timeout": colorize("⏱", Colors.YELLOW),
                            "error": colorize("⚠", Colors.YELLOW)
                        }.get(result.status, "?")
                        duration_color = Colors.DIM if result.status == "pass" else Colors.RESET
                        print(f"{status_symbol} {test_name} {colorize(f'({result.duration:.2f}s)', duration_color)}", file=sys.stderr)
                
                except Exception as e:
                    # Handle any errors in test execution
                    result = TestResult(
                        name=test_name,
                        status="error",
                        duration=0.0,
                        error=str(e)
                    )
                    results.append(result)
                    completed_count += 1
        
        # Clear progress line
        if not self.verbose:
            print(f"\r{' ' * 100}\r", end='', file=sys.stderr)
        
        duration = time.time() - start_time
        
        # Sort results by original test order for consistent output
        test_order = {name: idx for idx, name in enumerate(tests)}
        results.sort(key=lambda r: test_order.get(r.name, 999))
        
        passed = sum(1 for r in results if r.status == "pass")
        failed = sum(1 for r in results if r.status == "fail")
        timeout = sum(1 for r in results if r.status == "timeout")
        error = sum(1 for r in results if r.status == "error")
        
        # Print compact summary line after completion
        if not self.verbose:
            summary_parts = [f"Finished in {colorize(f'{duration:.2f}s', Colors.BRIGHT_CYAN)}"]
            
            if passed > 0:
                summary_parts.append(colorize(f'✓ {passed} passed', Colors.BRIGHT_GREEN))
            if failed > 0:
                summary_parts.append(colorize(f'✗ {failed} failed', Colors.BRIGHT_RED))
            if timeout > 0:
                summary_parts.append(colorize(f'⏱ {timeout} timeout', Colors.YELLOW))
            if error > 0:
                summary_parts.append(colorize(f'⚠ {error} error', Colors.YELLOW))
            
            print(": ".join(summary_parts), file=sys.stderr)
        
        return SuiteResult(
            suite_name=suite_name,
            total=len(results),
            passed=passed,
            failed=failed,
            timeout=timeout,
            error=error,
            duration=duration,
            tests=results
        )


def print_summary(suite_result: SuiteResult):
    """Print a human-readable summary of test results."""
    separator = colorize("=" * 70, Colors.BRIGHT_BLUE)
    
    print(f"\n{separator}")
    print(bold(f"Test Suite: {suite_result.suite_name}"))
    print(separator)
    
    total = suite_result.total
    passed = suite_result.passed
    failed = suite_result.failed
    timeout_count = suite_result.timeout
    error_count = suite_result.error
    pass_rate = passed / total * 100 if total > 0 else 0
    
    print(f"Total:    {total}")
    
    if passed > 0:
        print(f"Passed:   {colorize(str(passed), Colors.BRIGHT_GREEN)} ({pass_rate:.1f}%)")
    else:
        print(f"Passed:   {passed} ({pass_rate:.1f}%)")
    
    if failed > 0:
        print(f"Failed:   {colorize(str(failed), Colors.BRIGHT_RED)}")
    else:
        print(f"Failed:   {failed}")
    
    if timeout_count > 0:
        print(f"Timeout:  {colorize(str(timeout_count), Colors.YELLOW)}")
    else:
        print(f"Timeout:  {timeout_count}")
    
    if error_count > 0:
        print(f"Error:    {colorize(str(error_count), Colors.YELLOW)}")
    else:
        print(f"Error:    {error_count}")
    
    print(f"Duration: {colorize(f'{suite_result.duration:.2f}s', Colors.BRIGHT_CYAN)}")
    print(separator)
    
    if failed > 0:
        print(f"\n{colorize('Failed Tests:', Colors.BRIGHT_RED)}")
        for test in suite_result.tests:
            if test.status == "fail":
                print(f"  {colorize('✗', Colors.RED)} {test.name}")
    
    if timeout_count > 0:
        print(f"\n{colorize('Timeout Tests:', Colors.YELLOW)}")
        for test in suite_result.tests:
            if test.status == "timeout":
                print(f"  {colorize('⏱', Colors.YELLOW)} {test.name}")
    
    if error_count > 0:
        print(f"\n{colorize('Error Tests:', Colors.YELLOW)}")
        for test in suite_result.tests:
            if test.status == "error":
                print(f"  {colorize('⚠', Colors.YELLOW)} {test.name}")


def main():
    parser = argparse.ArgumentParser(
        description="Enhanced bash compatibility test runner for brush"
    )
    parser.add_argument("-b", "--bash-source", dest="bash_source_dir", required=True,
                        help="Path to bash source directory")
    parser.add_argument("-s", "--shell", dest="shell", required=True,
                        help="Path to shell to test")
    parser.add_argument("-t", "--timeout", type=int, default=30,
                        help="Timeout per test in seconds (default: 30)")
    parser.add_argument("-v", "--verbose", action="store_true",
                        help="Verbose output")
    parser.add_argument("-j", "--jobs", type=int, default=1,
                        help="Number of parallel jobs (default: 1 for sequential, 0 for auto-detect CPUs)")
    
    subparsers = parser.add_subparsers(dest="command", required=True)
    
    # List command
    list_parser = subparsers.add_parser("list", help="List available tests")
    list_parser.add_argument("--suites", action="store_true",
                             help="List available suites instead of tests")
    
    # Test command
    test_parser = subparsers.add_parser("test", help="Run a single test")
    test_parser.add_argument("test_name", help="Name of test to run")
    test_parser.add_argument("--show-output", action="store_true",
                             help="Show test output")
    
    # Suite command
    suite_parser = subparsers.add_parser("suite", help="Run test suite(s)")
    suite_parser.add_argument("suite_names", nargs="+", help="Name(s) of suite(s) to run (e.g., minimal, all)")
    suite_parser.add_argument("-f", "--filter", dest="filter",
                             help="Filter tests by name (substring match)")
    suite_parser.add_argument("--json", dest="json_output",
                             help="Write JSON results to file (for single suite) or directory (for multiple)")
    suite_parser.add_argument("-o", "--output-dir", dest="output_dir",
                             help="Output directory for results (default: current directory)")

    # Triage command
    triage_parser = subparsers.add_parser("triage", help="Quick triage of test failures from JSON")
    triage_parser.add_argument("json_file", help="JSON results file to analyze")
    triage_parser.add_argument("-n", "--num-lines", type=int, default=5,
                              help="Number of diff lines to show per test (default: 5)")
    
    args = parser.parse_args()
    
    try:
        runner = BashTestRunner(
            args.bash_source_dir,
            args.shell,
            timeout=args.timeout,
            verbose=args.verbose
        )
        
        if args.command == "list":
            if args.suites:
                suites = []
                for file in runner.tests_dir.glob("run-*"):
                    suite_name = file.name[4:]  # Remove "run-" prefix
                    if suite_name not in ["all", "minimal"]:
                        suites.append(suite_name)
                    else:
                        suites.insert(0, suite_name)
                for suite in sorted(suites):
                    print(suite)
            else:
                for test in runner.list_tests():
                    print(test)
        
        elif args.command == "test":
            result = runner.run_single_test(args.test_name)
            
            status_color = {
                "pass": Colors.BRIGHT_GREEN,
                "fail": Colors.BRIGHT_RED,
                "timeout": Colors.YELLOW,
                "error": Colors.YELLOW
            }.get(result.status, Colors.RESET)
            
            status_symbol = {
                "pass": "✓",
                "fail": "✗",
                "timeout": "⏱",
                "error": "⚠"
            }.get(result.status, "?")
            
            print(bold(f"Test: {result.name}"))
            print(f"Status: {colorize(status_symbol + ' ' + result.status, status_color)}")
            print(f"Duration: {colorize(f'{result.duration:.2f}s', Colors.BRIGHT_CYAN)}")

            if args.show_output:
                if result.output:
                    print(f"\n{bold('Output:')}\n{result.output}")
                else:
                    print(f"\n{bold('Output:')} {colorize('(empty)', Colors.DIM)}")

            if result.error:
                print(f"\n{colorize('Error/Diff:', Colors.BRIGHT_RED)}\n{result.error}")
            
            sys.exit(0 if result.status == "pass" else 1)
        
        elif args.command == "suite":
            # Auto-detect CPU count if jobs=0
            jobs = args.jobs
            if jobs == 0:
                jobs = cpu_count() or 4
                if not args.verbose:
                    print(colorize(f"Auto-detected {jobs} CPUs", Colors.DIM), file=sys.stderr)
            
            # Support multiple suites
            suite_results = []
            
            for suite_name in args.suite_names:
                if len(args.suite_names) > 1:
                    separator = colorize("=" * 70, Colors.BRIGHT_BLUE)
                    print(f"\n{separator}", file=sys.stderr)
                    print(bold(f"Running suite: {suite_name}"), file=sys.stderr)
                    print(separator, file=sys.stderr)
                
                suite_result = runner.run_suite(suite_name, test_filter=args.filter, jobs=jobs)
                suite_results.append(suite_result)
                
                # Print summary for each suite
                print_summary(suite_result)
                
                # Save JSON for individual suite
                if args.output_dir or (args.json_output and len(args.suite_names) > 1):
                    output_dir = Path(args.output_dir) if args.output_dir else Path.cwd()
                    output_dir.mkdir(exist_ok=True)
                    json_file = output_dir / f"{suite_name}.json"
                    with open(json_file, 'w') as f:
                        json.dump(asdict(suite_result), f, indent=2)
                    if args.verbose:
                        print(f"Results written to: {json_file}", file=sys.stderr)
            
            # For single suite with --json, save to specified file
            if len(args.suite_names) == 1 and args.json_output and not args.output_dir:
                with open(args.json_output, 'w') as f:
                    json.dump(asdict(suite_results[0]), f, indent=2)
                print(f"\nJSON results written to: {args.json_output}")
            
            # Print aggregate summary for multiple suites
            if len(suite_results) > 1:
                separator = colorize("=" * 70, Colors.BRIGHT_BLUE)
                print(f"\n{separator}")
                print(bold("Overall Summary"))
                print(separator)
                print(f"{bold('Suite'):<20} {bold('Total'):<10} {bold('Passed'):<10} {bold('Failed'):<10} {bold('Pass Rate'):<15}")
                print(colorize("-" * 70, Colors.DIM))
                
                total_total = 0
                total_passed = 0
                
                for suite_result in suite_results:
                    total = suite_result.total
                    passed = suite_result.passed
                    failed = suite_result.failed
                    pass_rate = passed / total * 100 if total > 0 else 0
                    
                    # Color the pass rate
                    if pass_rate == 100:
                        rate_str = colorize(f"{pass_rate:.1f}%", Colors.BRIGHT_GREEN)
                    elif pass_rate >= 50:
                        rate_str = colorize(f"{pass_rate:.1f}%", Colors.YELLOW)
                    else:
                        rate_str = colorize(f"{pass_rate:.1f}%", Colors.RED)
                    
                    passed_str = colorize(str(passed), Colors.BRIGHT_GREEN) if passed > 0 else str(passed)
                    failed_str = colorize(str(failed), Colors.BRIGHT_RED) if failed > 0 else str(failed)
                    
                    # Need to strip color codes for formatting
                    name_display = f"{suite_result.suite_name:<20}"
                    print(f"{name_display} {total:<10} {passed_str:<18} {failed_str:<18} {rate_str}")
                    
                    total_total += total
                    total_passed += passed
                
                print(colorize("-" * 70, Colors.DIM))
                overall_pass_rate = total_passed / total_total * 100 if total_total > 0 else 0
                
                if overall_pass_rate == 100:
                    rate_str = colorize(f"{overall_pass_rate:.1f}%", Colors.BRIGHT_GREEN)
                elif overall_pass_rate >= 50:
                    rate_str = colorize(f"{overall_pass_rate:.1f}%", Colors.YELLOW)
                else:
                    rate_str = colorize(f"{overall_pass_rate:.1f}%", Colors.RED)
                
                total_passed_str = colorize(str(total_passed), Colors.BRIGHT_GREEN) if total_passed > 0 else str(total_passed)
                total_failed = total_total - total_passed
                total_failed_str = colorize(str(total_failed), Colors.BRIGHT_RED) if total_failed > 0 else str(total_failed)
                
                print(f"{bold('TOTAL'):<20} {total_total:<10} {total_passed_str:<18} {total_failed_str:<18} {rate_str}")
                print(separator)
            
            # Exit with error if any suite had failures
            has_failures = any(
                sr.failed > 0 or sr.timeout > 0 or sr.error > 0
                for sr in suite_results
            )
            sys.exit(1 if has_failures else 0)

        elif args.command == "triage":
            # Load JSON results and show failure summaries
            with open(args.json_file) as f:
                data = json.load(f)

            suite_name = data.get("suite_name", "unknown")
            tests = data.get("tests", [])

            separator = colorize("=" * 70, Colors.BRIGHT_BLUE)
            print(separator)
            print(bold(f"Triage Report for Suite: {suite_name}"))
            print(separator)

            failed_tests = [t for t in tests if t["status"] == "fail"]
            timeout_tests = [t for t in tests if t["status"] == "timeout"]

            if not failed_tests and not timeout_tests:
                print(colorize("No failures or timeouts to triage!", Colors.BRIGHT_GREEN))
                sys.exit(0)

            # Show timeout tests first
            if timeout_tests:
                print(f"\n{colorize('TIMEOUT TESTS:', Colors.YELLOW)}")
                for test in timeout_tests:
                    print(f"\n  {colorize('⏱', Colors.YELLOW)} {bold(test['name'])} ({test['duration']:.2f}s)")
                    if test.get("error"):
                        print(f"    {colorize(test['error'], Colors.DIM)}")

            # Show failed tests with diff snippets
            if failed_tests:
                print(f"\n{colorize('FAILED TESTS:', Colors.BRIGHT_RED)}")

                for test in failed_tests:
                    print(f"\n  {colorize('✗', Colors.RED)} {bold(test['name'])} ({test['duration']:.2f}s)")

                    error = test.get("error", "")
                    if error:
                        # Show first N lines of diff
                        lines = error.split('\n')
                        # Skip diff header lines (---, +++, @@)
                        relevant_lines = []
                        for line in lines:
                            if line.startswith('---') or line.startswith('+++'):
                                continue
                            if line.startswith('@@'):
                                continue
                            relevant_lines.append(line)

                        # Show up to num_lines lines
                        shown_lines = relevant_lines[:args.num_lines]
                        for line in shown_lines:
                            if line.startswith('-'):
                                print(f"    {colorize(line, Colors.RED)}")
                            elif line.startswith('+'):
                                print(f"    {colorize(line, Colors.GREEN)}")
                            else:
                                print(f"    {colorize(line, Colors.DIM)}")

                        if len(relevant_lines) > args.num_lines:
                            remaining = len(relevant_lines) - args.num_lines
                            print(f"    {colorize(f'... ({remaining} more lines)', Colors.DIM)}")
                    else:
                        print(f"    {colorize('(no diff output)', Colors.DIM)}")

            print(f"\n{separator}")
            print(f"Total: {colorize(str(len(failed_tests)), Colors.BRIGHT_RED)} failed, " +
                  f"{colorize(str(len(timeout_tests)), Colors.YELLOW)} timeout")
            print(separator)

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
