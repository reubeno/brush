#!/usr/bin/python3
import argparse
import sys

parser = argparse.ArgumentParser(description='Format API diff report')
parser.add_argument("-p", dest="crate_name", type=str, help="Name of the crate", required=True)
parser.add_argument("diff_path", type=str, help="Path to API diff file from cargo-public-api")

args = parser.parse_args()

with open(args.diff_path, "r") as diff_file:
    diff_lines = diff_file.readlines()

removed_lines = []
added_lines = []
changed_lines = []

current_section = None

for line in diff_lines:
    line = line.strip()

    if "Removed items" in line:
        current_section = removed_lines
    elif "Added items" in line:
        current_section = added_lines
    elif "Changed items" in line:
        current_section = changed_lines
    elif "============" in line:
        continue
    elif line == "(none)":
        continue
    elif not line:
        continue
    elif current_section is not None:
        current_section.append(line)

if not removed_lines and not added_lines and not changed_lines:
    sys.stderr.write("note: no API changes detected\n")
    sys.exit(0)

print(f"# Public API changes for crate: {args.crate_name}")

if removed_lines:
    print()
    print("## Removed items")

    print("```")
    for line in removed_lines:
        print(line)
    print("```")

if added_lines:
    print()
    print("## Added items")

    print("```")
    for line in added_lines:
        print(line)
    print("```")

if changed_lines:
    print()
    print("## Changed items")

    print("```")
    for line in changed_lines:
        print(line)
    print("```")
