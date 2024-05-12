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
    deviation_in_ns: int

def parse_benchmarks_results(file_path: str) -> Dict[str, Benchmark]:
    benchmarks = {}

    with open(file_path, "r") as file:
        for line in file.readlines():
            match = re.match(r"test (.*) \.\.\. bench: +(\d+) ns/iter \(\+/- (\d+)\)", line.strip())
            if match:
                benchmark = Benchmark(
                    test_name=match.group(1),
                    duration_in_ns=int(match.group(2)),
                    deviation_in_ns=int(match.group(3))
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

if common:
    print(f"| {'Benchmark name':36} | {'Baseline (ns)':>13} | {'Test/PR (ns)':>13} | {'Delta (ns)':>13} | {'Delta %'} |")
    print(f"| {'-' * 36} | {'-' * 13} | {'-' * 13} | {'-' * 13} | {'-' * 7}")
    for name in sorted(common):
        base_duration = base_results[name].duration_in_ns
        test_duration = test_results[name].duration_in_ns

        delta_duration = test_duration - base_duration
        delta_str = str(delta_duration)
        if delta_duration > 0:
            delta_str = "+" + delta_str

        delta_percentage = (100.0 * delta_duration) / base_duration
        delta_percentage_str = f"{delta_percentage:.2f}%"
        if delta_percentage > 0:
            delta_percentage_str = "+" + delta_percentage_str

        print(f"| {name:36} | {base_duration:10} ns | {test_duration:10} ns | {delta_str:>10} ns | {delta_percentage_str:>7} |")

if removed_from_base:
    print("Benchmarks removed:")
    for name in removed_from_base:
        print(f"  - {name}")

if added_by_test:
    print("Benchmarks added:")
    for name in added_by_test:
        print(f"  - {name}")
