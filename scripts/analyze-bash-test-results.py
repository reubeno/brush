#!/usr/bin/env python3
"""
Analyze bash test results and provide insights.
"""
import argparse
import json
import sys
from collections import defaultdict
from typing import Dict, List


def analyze_results(results: dict):
    """Analyze test results and provide insights."""
    
    print(f"{'='*70}")
    print(f"Test Results Analysis: {results['suite_name']}")
    print(f"{'='*70}\n")
    
    # Group tests by status
    by_status = defaultdict(list)
    for test in results['tests']:
        by_status[test['status']].append(test)
    
    # Print passing tests
    if by_status['pass']:
        print(f"✓ PASSING TESTS ({len(by_status['pass'])}):")
        for test in sorted(by_status['pass'], key=lambda t: t['name']):
            print(f"  • {test['name']:<30} ({test['duration']:.2f}s)")
        print()
    
    # Print failing tests sorted by duration (quickest to slowest)
    if by_status['fail']:
        print(f"✗ FAILING TESTS ({len(by_status['fail'])}):")
        for test in sorted(by_status['fail'], key=lambda t: t['duration']):
            error_preview = ""
            if test.get('error'):
                # Get first line of error
                first_line = test['error'].split('\n')[0][:50]
                error_preview = f" - {first_line}"
            print(f"  • {test['name']:<30} ({test['duration']:.2f}s){error_preview}")
        print()
    
    # Print timeout tests
    if by_status['timeout']:
        print(f"⏱ TIMEOUT TESTS ({len(by_status['timeout'])}):")
        for test in sorted(by_status['timeout'], key=lambda t: t['name']):
            print(f"  • {test['name']:<30} (>{test['duration']:.0f}s)")
        print()
    
    # Print error tests
    if by_status['error']:
        print(f"⚠ ERROR TESTS ({len(by_status['error'])}):")
        for test in sorted(by_status['error'], key=lambda t: t['name']):
            error_msg = test.get('error', 'Unknown error')[:60]
            print(f"  • {test['name']:<30} - {error_msg}")
        print()
    
    # Statistics
    print(f"{'='*70}")
    print("STATISTICS")
    print(f"{'='*70}")
    
    total = results['total']
    passed = results['passed']
    failed = results['failed']
    timeout = results['timeout']
    error = results['error']
    
    print(f"Pass Rate:      {passed}/{total} ({passed/total*100:.1f}%)")
    print(f"Fail Rate:      {failed}/{total} ({failed/total*100:.1f}%)")
    if timeout > 0:
        print(f"Timeout Rate:   {timeout}/{total} ({timeout/total*100:.1f}%)")
    if error > 0:
        print(f"Error Rate:     {error}/{total} ({error/total*100:.1f}%)")
    
    # Average duration
    if results['tests']:
        avg_duration = sum(t['duration'] for t in results['tests']) / len(results['tests'])
        print(f"\nAvg Test Time:  {avg_duration:.2f}s")
        print(f"Total Time:     {results['duration']:.2f}s")
    
    print(f"{'='*70}")
    
    # Recommendations
    print("\nRECOMMENDATIONS:")
    if timeout > 0:
        print("  ⚠ Consider investigating timeout tests - they may indicate infinite loops")
    if failed > 0:
        failing_quick = [t for t in by_status['fail'] if t['duration'] < 1.0]
        if failing_quick:
            print(f"  💡 {len(failing_quick)} tests fail quickly - may be easier to debug")
    if passed > 0:
        print(f"  ✓ {passed} tests passing - good baseline for regression testing")


def main():
    parser = argparse.ArgumentParser(
        description="Analyze bash test results"
    )
    parser.add_argument("results_file", help="JSON results file to analyze")
    parser.add_argument("--show-errors", action="store_true",
                        help="Show full error details for failing tests")
    parser.add_argument("--filter-status", choices=['pass', 'fail', 'timeout', 'error'],
                        help="Only show tests with this status")
    
    args = parser.parse_args()
    
    with open(args.results_file) as f:
        results = json.load(f)
    
    if args.filter_status:
        # Filter tests
        filtered_tests = [t for t in results['tests'] if t['status'] == args.filter_status]
        results['tests'] = filtered_tests
        results['total'] = len(filtered_tests)
    
    analyze_results(results)
    
    if args.show_errors:
        failing = [t for t in results['tests'] if t['status'] == 'fail']
        if failing:
            print(f"\n{'='*70}")
            print("DETAILED ERRORS")
            print(f"{'='*70}\n")
            for test in failing:
                print(f"Test: {test['name']}")
                print(f"Duration: {test['duration']:.2f}s")
                if test.get('error'):
                    print("Error:")
                    print(test['error'])
                print(f"{'-'*70}\n")


if __name__ == "__main__":
    main()
