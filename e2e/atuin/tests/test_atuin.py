"""atuin shell integration: bash.

Written to be upstreamable: nothing here knows which shell it is driving beyond
`$ATUIN_TEST_BASH`, and every assertion is about atuin's own observable behavior.
"""

import os
import re
import subprocess
import time

import pytest
from rcfile import write_rc
from tmux_shell import TIMEOUT, TmuxShell


PROMPT = "atuin-test>"


def atuin(*args):
    """Runs atuin out of band, against the same database the shell under test is using."""
    # atuin insists on a session id even just to list.
    env = os.environ | {"ATUIN_SESSION": os.environ.get("ATUIN_SESSION") or uuid()}
    return subprocess.run(
        ["atuin", *args], env=env, capture_output=True, text=True, check=False
    ).stdout


def uuid():
    return subprocess.run(["atuin", "uuid"], capture_output=True, text=True, check=True).stdout.strip()


def wait_for_history(pattern):
    """Waits until `atuin history list` matches. atuin records `history end` asynchronously."""
    search = re.compile(pattern, re.MULTILINE).search
    deadline = time.monotonic() + TIMEOUT
    while True:
        listing = atuin("history", "list", "--format", "{exit} {command}")
        if search(listing):
            return
        assert time.monotonic() < deadline, (
            f"timed out waiting for history /{pattern}/; history:\n{listing}"
        )
        time.sleep(0.1)


@pytest.fixture
def shell(tmp_path, monkeypatch):
    # Isolate atuin's config and database per test.
    home = tmp_path / "home"
    home.mkdir()
    monkeypatch.setenv("HOME", str(home))
    monkeypatch.setenv("TMUX_TMPDIR", str(tmp_path))
    for name in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "ATUIN_SESSION", "ATUIN_HISTORY_ID", "TMUX"):
        monkeypatch.delenv(name, raising=False)

    # Create and migrate the database once, up front: atuin migrates on first use, and two atuin
    # processes racing to migrate a fresh database trip a UNIQUE constraint. Our out-of-band
    # history queries would otherwise race the shell's own atuin at startup.
    atuin("history", "list")

    rc = write_rc(tmp_path / "rc", PROMPT, 'eval "$(atuin init bash)"')
    shell = TmuxShell(tmp_path / "tmux", os.environ["ATUIN_TEST_BASH"], rc, PROMPT)
    yield shell
    shell.close()


def test_init_binds_ctrl_r_and_up_arrow(shell, tmp_path):
    macros = tmp_path / "macros"
    shell.type_line(f"bind -s > {macros}")
    bindings = macros.read_text()
    assert r'"\C-r"' in bindings
    assert r'"\e[A"' in bindings


def test_commands_are_recorded_with_their_exit_status(shell):
    shell.type_line("echo recorded-one")
    shell.wait_for("^recorded-one$")
    shell.type_line("false")
    # An empty command, so atuin's precmd fires `history end` for `false`, whose exit status it
    # only records on the following prompt.
    shell.type_line("")
    wait_for_history("^0 echo recorded-one$")
    wait_for_history("^1 false$")


def test_ctrl_r_search_runs_the_selected_command(shell):
    shell.type_line("echo needle-in-history")
    shell.wait_for("^needle-in-history$")
    shell.type_line("")
    wait_for_history("^0 echo needle-in-history$")
    shell.send_key("C-r")
    shell.wait_for("Atuin v")
    shell.send_text("needle-in")
    shell.wait_for(r"^\s*>.*echo needle-in-history")
    shell.send_key("Enter")
    shell.wait_for_prompt()
    # The command was run again: two outputs on screen.
    shell.wait_for_screen_count("^needle-in-history$", 2)


def test_escaping_ctrl_r_search_keeps_the_current_line(shell):
    shell.send_text("echo keep-me")
    shell.send_key("C-r")
    shell.wait_for("Atuin v")
    shell.send_key("Escape")
    shell.wait_for(r"^atuin-test> echo keep-me$")


def test_up_arrow_opens_search(shell):
    shell.type_line("echo up-arrow-entry")
    shell.wait_for("^up-arrow-entry$")
    shell.send_key("Up")
    shell.wait_for("Atuin v")
    shell.send_key("Escape")
    shell.wait_for_prompt()
