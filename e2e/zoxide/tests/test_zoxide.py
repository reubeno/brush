import os
import shlex

import pytest
from pty_shell import run, spawn
from rcfile import write_rc


PROMPT = "zoxide-test> "


@pytest.fixture
def shell(tmp_path):
    home = tmp_path / "home"
    data = tmp_path / "data"
    marker = tmp_path / "prompt-command"
    home.mkdir()
    data.mkdir()
    marker.touch()
    rc = write_rc(
        tmp_path / "rc",
        PROMPT.rstrip(),
        'PROMPT_COMMAND=\'echo ran >> "$ZOXIDE_TEST_MARKER"\'',
        'eval "$(zoxide init bash)"',
    )
    env = os.environ | {
        "HOME": str(home),
        "TERM": "dumb",
        "ZOXIDE_TEST_MARKER": str(marker),
        "_ZO_DATA_DIR": str(data),
    }
    child = spawn(
        os.environ["ZOXIDE_TEST_SHELL"],
        ["--noprofile", "--rcfile", str(rc)],
        env=env,
        prompt=PROMPT,
    )
    yield child, home, marker
    child.close(force=True)


def test_init_defines_z(shell):
    child, _, _ = shell
    assert "function\r\n" in run(child, PROMPT, "type -t z")


def test_prompt_hook_preserves_status(shell):
    child, _, _ = shell
    run(child, PROMPT, "false")
    assert "1\r\n" in run(child, PROMPT, "echo $?")


def test_prompt_hook_preserves_existing_command(shell):
    child, _, marker = shell
    before = marker.read_text().count("ran")
    run(child, PROMPT, "true")
    assert marker.read_text().count("ran") > before


def test_visited_directory_can_be_recalled(shell):
    child, home, _ = shell
    target = home / "projects" / "zoxide-target"
    target.mkdir(parents=True)
    run(child, PROMPT, f"cd {shlex.quote(str(target))}")
    run(child, PROMPT, f"cd {shlex.quote(str(home))}")
    run(child, PROMPT, "z zoxide-target")
    assert f"{target}\r\n" in run(child, PROMPT, "pwd")
