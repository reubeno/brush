#!/usr/bin/python3
import argparse
import json

parser = argparse.ArgumentParser(description='Summarize pytest results')
parser.add_argument("-r", "--results", dest="results_file_path", type=str, required=True, help="Path to .json pytest results file")
parser.add_argument("--title", dest="title", type=str, default="Pytest results", help="Title to display")

args = parser.parse_args()

with open(args.results_file_path, "r") as results_file:
    results = json.load(results_file)

summary = results["summary"]

error_count = summary.get("error") or 0
fail_count = summary.get("failed") or 0
pass_count = summary.get("passed") or 0
skip_count = summary.get("skipped") or 0
expected_fail_count = summary.get("xfailed") or 0
unexpected_pass_count = summary.get("xpassed") or 0

total_count = summary.get("total") or 0
collected_count = summary.get("collected") or 0
deselected_count = summary.get("deselected") or 0

#
# Output
#

print(f"# {args.title}")

print(f"| Outcome            | Count                   | Percentage |")
print(f"| ------------------ | ----------------------: | ---------: |")
print(f"| ✅ Pass            | {pass_count}            | <span style='color:green'>{pass_count * 100 / total_count:.2f}</span> |")

if error_count > 0:
    print(f"| ❗️ Error           | {error_count}           | <span style='color:red'>{error_count * 100 / total_count:.2f}</span> |")
if fail_count > 0:
    print(f"| ❌ Fail            | {fail_count}            | <span style='color:red'>{fail_count * 100 / total_count:.2f}</span> |")
if skip_count > 0:
    print(f"| ⏩ Skip            | {skip_count}            | {skip_count * 100 / total_count:.2f} |")
if expected_fail_count > 0:
    print(f"| ❎ Expected Fail   | {expected_fail_count}   | {expected_fail_count * 100 / total_count:.2f} |")
if unexpected_pass_count > 0:
    print(f"| ✔️ Unexpected Pass | {unexpected_pass_count} | <span style='color:red'>{unexpected_pass_count * 100 / total_count:.2f}</span> |")

print(f"| 📊 Total           | {total_count}           | {total_count * 100 / total_count:.2f} |")
