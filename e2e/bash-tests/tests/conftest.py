import re
from pathlib import Path


def pytest_addoption(parser):
    parser.addoption(
        "--subset",
        type=suite_members,
        help="run only the scripts an upstream suite runs, e.g. `minimal` for run-minimal",
    )


def suite_members(name):
    """The run scripts named in the `run-X|run-Y) echo $x ; sh $x ;;` arms of tests/run-<name>."""
    members = set()
    for line in (Path("/bash/tests") / f"run-{name}").read_text().splitlines():
        if "sh $x" in line and ")" in line:
            members.update(re.findall(r"run-([\w-]+)", line.split(")")[0]))
    return members
