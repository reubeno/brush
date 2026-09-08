import logging
import os
import re
import shutil
import signal
import subprocess
import time
from pathlib import Path

import pytest
from e2e_skip import parse


# ble.sh has no list of test sections: `ble.sh --test <name>` just sources lib/test-<name>.sh.
# Its GNUmakefile decides which of those are sections by building them into out/lib, leaving
# include-only data (lib/test-canvas.GraphemeClusterTest.sh) behind. Reading the built directory
# therefore tracks upstream: a section added by a version bump runs instead of being missed.
SECTIONS = Path("/blesh/out/lib")
RESULTS = Path("/results")
RUN_DIR = RESULTS / "run"
PROGRESS_DIR = RESULTS / "progress"
ANSI = re.compile(r"\x1b\[[0-9;]*[a-zA-Z]")
SUMMARY = re.compile(
    r"\[section\] (.+?): (\d+)/(\d+) \((\d+) fail, (\d+) crash, (\d+) skip\)"
)
LOG = logging.getLogger(__name__)


def selected_files():
    available = sorted(path.stem.removeprefix("test-") for path in SECTIONS.glob("test-*.sh"))
    if not available:
        raise RuntimeError(f"no ble.sh test sections under {SECTIONS}")
    requested = [line for line in os.environ.get("BLESH_TEST_FILES", "").splitlines() if line]
    if unknown := sorted(set(requested) - set(available)):
        raise RuntimeError(
            f"unknown ble.sh test section(s): {', '.join(unknown)}; "
            f"available: {', '.join(available)}"
        )
    # The skip list applies to an explicit selection too: a section that hangs hangs either way.
    # Parsed here rather than by the shared pytest plugin: a section must be dropped before it is
    # parametrized, or ble.sh would still be run for it.
    skipped = parse(os.environ.get("E2E_SKIP_LIST", ""))
    return [name for name in requested or available if name not in skipped]


def save_progress(name):
    saved = []
    for source in (RUN_DIR / "blesh").glob("*.test/*"):
        if not source.name.isdigit():
            continue
        destination = PROGRESS_DIR / f"test-{name}" / source.name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
        saved.append(str(destination))
    return saved


@pytest.fixture(scope="session", autouse=True)
def clean_artifacts():
    shutil.rmtree(PROGRESS_DIR, ignore_errors=True)


@pytest.mark.parametrize("name", selected_files())
def test_upstream_file(name):
    shutil.rmtree(RUN_DIR / "blesh", ignore_errors=True)
    started = time.monotonic()
    process = subprocess.Popen(
        [os.environ["BLESH_TEST_SHELL"], "ble.sh", "--test", name],
        cwd="/blesh/out",
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        errors="replace",
        start_new_session=True,
    )
    timed_out = False
    process_tree = ""
    try:
        timeout = int(os.environ.get("BLESH_TEST_TIMEOUT", "180"))
        output, _ = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        process_tree = subprocess.run(
            [
                "ps",
                "-s",
                str(process.pid),
                "-o",
                "pid,stat,etime,wchan:24,args",
                "--forest",
            ],
            capture_output=True,
            text=True,
            check=False,
        ).stdout
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            LOG.debug("process group %s no longer exists during timeout cleanup", process.pid)
        try:
            # Bounded: a descendant that escaped the process group would otherwise hold the pipe
            # open for as long as it lives, turning one hung section into a hung suite.
            output, _ = process.communicate(timeout=30)
        except subprocess.TimeoutExpired:
            process.kill()
            output = "<output unavailable: a descendant survived the process group kill>"

    progress = save_progress(name)
    LOG.info(
        "==> test-%s\n%s\n%s==> test-%s (%.1fs) exit=%s%s",
        name,
        output,
        f"process tree:\n{process_tree}" if process_tree else "",
        name,
        time.monotonic() - started,
        process.returncode,
        f"\nsaved progress: {', '.join(progress)}" if progress else "",
    )

    summaries = SUMMARY.findall(ANSI.sub("", output))
    failed = [
        f"{section}: {passed}/{total} ({failures} fail, {crashes} crash)"
        for section, passed, total, failures, crashes, _ in summaries
        if passed != total or failures != "0" or crashes != "0"
    ]
    problems = []
    if timed_out:
        problems.append("timed out")
    elif process.returncode:
        problems.append(f"exited with status {process.returncode}")
    if not summaries:
        problems.append("emitted no section summary")
    problems.extend(failed)
    assert not problems, "; ".join(problems)
