#!/usr/bin/python3
import argparse
import datetime
import re
from dataclasses import dataclass
from typing import Dict

parser = argparse.ArgumentParser()
parser.add_argument("-b", "--base-results", dest="base_results_file_path", type=str, help="Path to base results output file", required=True)
parser.add_argument("-t", "--test-results", dest="test_results_file_path", type=str, help="Path to test results output file", required=True)

args = parser.parse_args()

@dataclass
class Benchmark:
    test_name: str
    duration_in_ns: int
    plus_or_minus_in_ns: int

def parse_benchmarks_results(file_path: str) -> Dict[str, Benchmark]:
    benchmarks = {}

    with open(file_path, "r") as file:
        for line in file.readlines():
            match = re.match(r"test (.*) \.\.\. bench: +(\d+) ns/iter \(\+/- (\d+)\)", line.strip())
            if match:
                benchmark = Benchmark(
                    test_name=match.group(1),
                    duration_in_ns=int(match.group(2)),
                    plus_or_minus_in_ns=int(match.group(3))
                )

                benchmarks[benchmark.test_name] = benchmark

    return benchmarks

base_results = parse_benchmarks_results(args.base_results_file_path)
test_results = parse_benchmarks_results(args.test_results_file_path)

base_test_names = set(base_results.keys())
test_test_names = set(test_results.keys())

removed_from_base = base_test_names - test_test_names
added_by_test = test_test_names - base_test_names
common = base_test_names & test_test_names

print("# Performance Benchmark Report")

if common:
    print(f"| {'Benchmark name':38} | {'Baseline (μs)':>13} | {'Test/PR (μs)':>13} | {'Delta (μs)':>13} | {'Delta %':15} |")
    print(f"| {'-' * 38} | {'-' * 13} | {'-' * 13} | {'-' * 13} | {'-' * 15} |")
    for name in sorted(common):
        # Retrieve base data
        base_duration = base_results[name].duration_in_ns / 1000.0
        base_plus_or_minus = base_results[name].plus_or_minus_in_ns / 1000.0
        base_plus_or_minus_percentage = (100.0 * base_plus_or_minus) / base_duration

        # Retrieve test data
        test_duration = test_results[name].duration_in_ns / 1000.0
        test_plus_or_minus = test_results[name].plus_or_minus_in_ns / 1000.0
        test_plus_or_minus_percentage = (100.0 * test_plus_or_minus) / test_duration

        # Compute delta
        delta_duration = test_duration - base_duration
        delta_percentage = (100.0 * delta_duration) / base_duration
        abs_delta_percentage = abs(delta_percentage)
        max_plus_or_minus_percentage = max(base_plus_or_minus_percentage, test_plus_or_minus_percentage)

        # Format
        delta_str = f"{delta_duration:8.2f}"

        if abs_delta_percentage > max_plus_or_minus_percentage:
            if delta_percentage < 0:
                delta_prefix = "🟢 "
            elif delta_percentage > 0:
                delta_prefix = "🟠 +"
            else:
                delta_prefix = "⚪  "

            delta_percentage_str = f"{delta_prefix}{delta_percentage:.2f}%"
        else:
            delta_percentage_str = "⚪  Unchanged"

        print(f"| `{name:36}` | `{base_duration:8.2f} μs` | `{test_duration:8.2f} μs` | `{delta_str:>8} μs` | `{delta_percentage_str:12}` |")

if removed_from_base:
    print()
    print("Benchmarks removed:")
    for name in removed_from_base:
        print(f"  - {name}")

if added_by_test:
    print()
    print("Benchmarks added:")
    for name in added_by_test:
        print(f"  - {name}")
