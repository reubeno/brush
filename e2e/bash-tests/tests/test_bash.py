"""Bash's own regression suite (tests/run-*), one pytest case per run script.

Each upstream `run-X` script runs `${THIS_SH} ./X.tests > ${BASH_TSTOUT}` and then diffs that
against `X.right`. Rather than re-parse those command lines, a case sources the script verbatim
with `diff` defined as a shell function that records what it was asked to compare; the comparison
itself happens here, after the shell's name is normalized out of its error messages.
"""

import difflib
import os
import resource
import shutil
import signal
import subprocess
import threading
from pathlib import Path

import pytest

TESTS = Path("/bash/tests")
RESULTS = Path("/results/bash-tests")
THIS_SH = os.environ["THIS_SH"]

# Scripts that need a controlling terminal: without one they print almost nothing (exec), or take
# a different path because isatty() fails (read -e, test -t, history, {var}> on /dev/fd). Not
# everything runs on one: `jobs` would then have `fg` block on its stopped jobs until they finish.
# execscript starts `bash -i`, which complains about job control without one.
PTY_TESTS = {"exec", "execscript", "history", "read", "test", "vredir"}
TIMEOUT = 30
# `jobs` sleeps and waits for about a minute in total.
SLOW_TIMEOUTS = {"jobs": 4 * TIMEOUT}

# Output that differs on Linux for bash itself, keyed by run script: a test whose every differing
# hunk is a small one around a line with one of its markers passes. Any other hunk still fails it,
# so these cannot mask a regression elsewhere in the same test.
PLATFORM_DIFFS = {
    # exec.right lists traps in the signal numbering where SIGTERM comes before SIGUSR1.
    "execscript": ["trap -- "],
    # glibc's printf reports this overflow, where .right expects it not to.
    "printf": ["Value too large"],
}

# `ulimit -u` beyond the hard limit is an expected error in builtins11.sub, but a container's
# limit is typically unlimited; give the tests a finite one.
soft, hard = resource.getrlimit(resource.RLIMIT_NPROC)
if hard == resource.RLIM_INFINITY:
    hard = 1 << 20
    # RLIM_INFINITY is -1 here, so min() would keep it.
    resource.setrlimit(resource.RLIMIT_NPROC, (hard if soft == resource.RLIM_INFINITY else min(soft, hard), hard))

# `diff` as the run scripts call it: [flags] actual expected, numbered because run-dirstack makes
# two. The `diff -a x x` probe some scripts make, to see whether -a is supported, compares a file
# with itself and is not recorded.
CAPTURE_DIFF = r"""
diff() {
    while [ "${1#-}" != "$1" ]; do shift; done
    [ "$1" = "$2" ] && return 0
    CAPTURED=$((${CAPTURED:-0} + 1))
    cp "$1" "$CAPTURE_DIR/$CAPTURED.actual"
    printf '%s\n' "$2" > "$CAPTURE_DIR/$CAPTURED.right"
}
"""


def run_scripts():
    # run-all and run-minimal run the others; run-gprof is not a test.
    skip = {"run-all", "run-minimal", "run-gprof"}
    return sorted(
        p.name.removeprefix("run-")
        for p in TESTS.glob("run-*")
        if p.name not in skip and not p.name.endswith(("~", ".orig"))
    )


def normalize(output):
    """Report the shell as `bash`, as the `.right` files do, whatever its path or name."""
    name = Path(THIS_SH).name
    lines = []
    for line in output.replace(f"{THIS_SH}:", "bash:").splitlines():
        if line.startswith(f"{name}:"):
            line = "bash:" + line.removeprefix(f"{name}:")
        lines.append(line)
    return lines


def explained(script, expected, actual):
    markers = PLATFORM_DIFFS.get(script)
    if not markers:
        return False
    for hunk in difflib.SequenceMatcher(None, expected, actual).get_grouped_opcodes():
        changed = [
            line
            for tag, i1, i2, j1, j2 in hunk
            if tag != "equal"
            for line in expected[i1:i2] + actual[j1:j2]
        ]
        marked = sum(any(marker in line for marker in markers) for line in changed)
        # ponytail: the known differences are a marker line plus at most a couple of lines it
        # displaces; anything larger is a real difference that happens to mention a marker.
        if not marked or len(changed) > 3 * marked:
            return False
    return True


def run(script, cwd, env, timeout, pty):
    command = ["sh", "-c", f'{CAPTURE_DIFF}\n. ./"$1"', "sh", f"run-{script}"]
    if not pty:
        process = subprocess.Popen(
            command, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True,
        )
        return wait(process, timeout)

    import fcntl
    import termios

    master, slave = os.openpty()

    def controlling_terminal():
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)

    process = subprocess.Popen(
        command, cwd=cwd, env=env, stdin=slave, stdout=slave, stderr=slave,
        preexec_fn=controlling_terminal,
    )
    os.close(slave)
    chunks = []

    def drain():
        # EIO once the last holder of the slave side has gone.
        try:
            while chunk := os.read(master, 65536):
                chunks.append(chunk)
        except OSError:
            pass

    reader = threading.Thread(target=drain, daemon=True)
    reader.start()
    try:
        wait(process, timeout)
    finally:
        reader.join(timeout=5)
        os.close(master)
    return b"".join(chunks)


def wait(process, timeout):
    try:
        return process.communicate(timeout=timeout)[0]
    except subprocess.TimeoutExpired:
        # The shell under test, and anything it started, is in the process group.
        os.killpg(process.pid, signal.SIGKILL)
        process.communicate()
        pytest.fail(f"timed out after {timeout}s", pytrace=False)


@pytest.mark.parametrize("script", run_scripts())
def test_bash(script, tmp_path, request):
    if (subset := request.config.getoption("subset")) and script not in subset:
        pytest.skip("not in --subset")
    # A fresh copy per case: the scripts write into their working directory and TMPDIR.
    work = tmp_path / "tests"
    shutil.copytree(TESTS, work, symlinks=True)
    capture = tmp_path / "capture"
    capture.mkdir()
    tmp = tmp_path / "tmp"
    tmp.mkdir()
    (tmp_path / "home").mkdir()
    # What `make tests` and run-all export; BASH_ENV, SHELLOPTS and BASHOPTS they remove. The
    # tests dir goes on PATH by absolute path too, for recho & co. after a test changes directory.
    # HOME must not be /tmp, as the container has it: the dirstack tests expect /tmp not to read
    # back as `~`.
    env = {k: v for k, v in os.environ.items() if k not in ("BASH_ENV", "SHELLOPTS", "BASHOPTS")}
    env.update(
        THIS_SH=THIS_SH, BUILD_DIR="/bash", TMPDIR=str(tmp), PATH=f".:{work}:{env['PATH']}",
        HOME=str(tmp_path / "home"),
        BASH_TSTOUT=str(tmp_path / "tstout"), CAPTURE_DIR=str(capture),
    )
    runner_output = run(
        script, work, env, SLOW_TIMEOUTS.get(script, TIMEOUT), script in PTY_TESTS
    ).decode(errors="replace")
    if runner_output:
        print(runner_output)

    captures = sorted(capture.glob("*.right"), key=lambda p: int(p.stem))
    assert captures, f"run-{script} never compared its output"
    failures = []
    for captured in captures:
        right = captured.read_text().strip()
        expected = (work / right).read_bytes().decode(errors="replace").splitlines()
        actual = normalize(
            captured.with_suffix(".actual").read_bytes().decode(errors="replace")
        )
        diff = "\n".join(difflib.unified_diff(expected, actual, right, "actual", lineterm=""))
        if not diff:
            continue
        if explained(script, expected, actual):
            print(f"{right}: differences explained by PLATFORM_DIFFS:\n{diff}")
            continue
        RESULTS.mkdir(parents=True, exist_ok=True)
        stem = Path(right).stem
        (RESULTS / f"{stem}.actual").write_text("\n".join(actual) + "\n")
        (RESULTS / f"{stem}.diff").write_text(diff + "\n")
        failures.append(f"output differs from {right}:\n{diff}")
    if failures:
        pytest.fail("\n".join(failures), pytrace=False)
