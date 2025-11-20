#!/usr/bin/env python3
"""
Compare bash test results across multiple runs or versions.
"""
import argparse
import json
import sys
from pathlib import Path
from typing import Dict, List


def load_results(filepath: str) -> dict:
    """Load test results from JSON file."""
    with open(filepath) as f:
        return json.load(f)


def print_comparison(baseline: dict, current: dict):
    """Print a comparison of two test runs."""
    print(f"{'='*70}")
    print(f"Test Results Comparison")
    print(f"{'='*70}")
    print(f"{'Metric':<30} {'Baseline':<15} {'Current':<15} {'Change':<10}")
    print(f"{'-'*70}")
    
    # Overall stats
    metrics = [
        ("Total Tests", "total"),
        ("Passed", "passed"),
        ("Failed", "failed"),
        ("Timeout", "timeout"),
        ("Error", "error"),
    ]
    
    for label, key in metrics:
        baseline_val = baseline[key]
        current_val = current[key]
        change = current_val - baseline_val
        change_str = f"{change:+d}" if change != 0 else "="
        
        if key == "passed":
            # For passed tests, positive is good
            if change > 0:
                change_str = f"✓ {change_str}"
            elif change < 0:
                change_str = f"✗ {change_str}"
        elif key in ["failed", "timeout", "error"]:
            # For failures, negative is good
            if change < 0:
                change_str = f"✓ {change_str}"
            elif change > 0:
                change_str = f"✗ {change_str}"
        
        print(f"{label:<30} {baseline_val:<15} {current_val:<15} {change_str:<10}")
    
    # Pass rate
    baseline_rate = baseline['passed'] / baseline['total'] * 100 if baseline['total'] > 0 else 0
    current_rate = current['passed'] / current['total'] * 100 if current['total'] > 0 else 0
    rate_change = current_rate - baseline_rate
    
    print(f"{'-'*70}")
    print(f"{'Pass Rate':<30} {baseline_rate:<15.1f}% {current_rate:<15.1f}% {rate_change:+.1f}%")
    print(f"{'='*70}")
    
    # Detailed changes
    baseline_tests = {t['name']: t['status'] for t in baseline['tests']}
    current_tests = {t['name']: t['status'] for t in current['tests']}
    
    # Find tests that changed status
    fixed = []
    regressed = []
    new_failures = []
    new_passes = []
    
    for name in current_tests:
        current_status = current_tests[name]
        baseline_status = baseline_tests.get(name)
        
        if baseline_status is None:
            # New test
            if current_status == "pass":
                new_passes.append(name)
            else:
                new_failures.append(name)
        elif baseline_status != current_status:
            if baseline_status != "pass" and current_status == "pass":
                fixed.append(name)
            elif baseline_status == "pass" and current_status != "pass":
                regressed.append(name)
    
    if fixed:
        print(f"\n✓ Tests Fixed ({len(fixed)}):")
        for name in sorted(fixed):
            print(f"  - {name}")
    
    if regressed:
        print(f"\n✗ Tests Regressed ({len(regressed)}):")
        for name in sorted(regressed):
            print(f"  - {name}")
    
    if new_passes:
        print(f"\n+ New Passing Tests ({len(new_passes)}):")
        for name in sorted(new_passes):
            print(f"  - {name}")
    
    if new_failures:
        print(f"\n- New Failing Tests ({len(new_failures)}):")
        for name in sorted(new_failures):
            print(f"  - {name}")


def print_summary_table(results: Dict[str, dict]):
    """Print a summary table of multiple test runs."""
    print(f"{'='*90}")
    print(f"Test Results Summary")
    print(f"{'='*90}")
    print(f"{'Run':<20} {'Total':<10} {'Passed':<10} {'Failed':<10} {'Timeout':<10} {'Pass Rate':<15}")
    print(f"{'-'*90}")
    
    for name, result in results.items():
        total = result['total']
        passed = result['passed']
        failed = result['failed']
        timeout = result['timeout']
        pass_rate = passed / total * 100 if total > 0 else 0
        
        print(f"{name:<20} {total:<10} {passed:<10} {failed:<10} {timeout:<10} {pass_rate:<15.1f}%")
    
    print(f"{'='*90}")


def main():
    parser = argparse.ArgumentParser(
        description="Compare bash test results"
    )
    
    subparsers = parser.add_subparsers(dest="command", required=True)
    
    # Compare command
    compare_parser = subparsers.add_parser("compare", help="Compare two test runs")
    compare_parser.add_argument("baseline", help="Baseline results JSON file")
    compare_parser.add_argument("current", help="Current results JSON file")
    
    # Summary command
    summary_parser = subparsers.add_parser("summary", help="Show summary of multiple runs")
    summary_parser.add_argument("results", nargs="+", help="Results JSON files")
    summary_parser.add_argument("--names", nargs="+", help="Names for each run (default: filenames)")
    
    args = parser.parse_args()
    
    if args.command == "compare":
        baseline = load_results(args.baseline)
        current = load_results(args.current)
        print_comparison(baseline, current)
    
    elif args.command == "summary":
        results = {}
        for i, filepath in enumerate(args.results):
            if args.names and i < len(args.names):
                name = args.names[i]
            else:
                name = Path(filepath).stem
            results[name] = load_results(filepath)
        
        print_summary_table(results)


if __name__ == "__main__":
    main()
