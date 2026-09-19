import re
from pathlib import Path


def pytest_addoption(parser):
    parser.addoption(
        "--subset",
        type=suite_members,
        help="run only the scripts an upstream suite runs, e.g. `minimal` for run-minimal",
    )
    parser.addoption(
        "--script-timeout",
        type=int,
        default=30,
        help="seconds each run script may take (default 30; slow scripts get a multiple)",
    )


def suite_members(name):
    """The run scripts named in the `run-X|run-Y) echo $x ; sh $x ;;` arms of tests/run-<name>."""
    members = set()
    for line in (Path("/bash/tests") / f"run-{name}").read_text().splitlines():
        if "sh $x" in line and ")" in line:
            members.update(re.findall(r"run-([\w-]+)", line.split(")")[0]))
    return members


# Expected lines reproduced across the run, summed from each script's properties. Pass/fail per
# script hides progress within one; this shows it. Scripts that time out report no counts.
lines = {"matched_lines": 0, "expected_lines": 0}


def pytest_runtest_logreport(report):
    # With xdist this runs in the controller, which receives each worker's user_properties.
    if report.when == "call":
        for name, value in report.user_properties:
            if name in lines:
                lines[name] += value


def pytest_terminal_summary(terminalreporter):
    matched, expected = lines["matched_lines"], lines["expected_lines"]
    if expected:
        terminalreporter.write_line(
            f"expected lines matched: {matched}/{expected} ({100 * matched / expected:.1f}%)"
        )
