import os

import pytest
from pty_shell import run, spawn
from rcfile import write_rc


# starship is configured (below) to render exactly this, so the prompt doubles as the assertion:
# `$status` is empty after a success and carries the exit status after a failure.
PROMPT = "STARSHIP> "
FAILED_PROMPT = "STARSHIP1> "

CONFIG = """\
add_newline = false
format = 'STARSHIP$status> '
[status]
disabled = false
format = '$status'
map_symbol = false
"""


@pytest.fixture
def shell(tmp_path):
    home = tmp_path / "home"
    home.mkdir()
    marker = tmp_path / "prompt-command"
    marker.touch()
    config = tmp_path / "starship.toml"
    config.write_text(CONFIG)
    # starship overwrites this prompt at its first render, so the tests wait on PROMPT instead.
    rc = write_rc(
        tmp_path / "rc",
        "loading>",
        'PROMPT_COMMAND=\'echo ran >> "$STARSHIP_TEST_MARKER"\'',
        'eval "$(starship init bash)"',
    )
    env = os.environ | {
        "HOME": str(home),
        # starship disables itself under TERM=dumb, so this suite needs a real terminal type;
        # pty_shell answers the cursor-position queries the prompt then makes.
        "TERM": "xterm-256color",
        "STARSHIP_CONFIG": str(config),
        "STARSHIP_TEST_MARKER": str(marker),
    }
    # spawn waits for the first rendered prompt, so every test starts from a shell that has
    # already run its init -- and a starship that never renders fails here rather than leaving a
    # later assertion to pass against the prompt that was on screen all along.
    child = spawn(
        os.environ["STARSHIP_TEST_SHELL"],
        ["--noprofile", "--rcfile", str(rc)],
        env=env,
        prompt=PROMPT,
    )
    yield child, marker
    child.close(force=True)


def test_init_renders_the_prompt(shell):
    child, _ = shell
    assert "starship" in run(child, PROMPT, "type -t starship_precmd")


def test_failed_command_status_reaches_the_prompt(shell):
    child, _ = shell
    # Waiting for a prompt that differs from the one already on screen is what makes this a real
    # assertion: it cannot be satisfied by the state the test started in.
    run(child, FAILED_PROMPT, "false")


def test_status_clears_after_a_successful_command(shell):
    child, _ = shell
    run(child, FAILED_PROMPT, "false")
    run(child, PROMPT, "true")


def test_existing_prompt_command_still_runs(shell):
    child, marker = shell
    before = marker.read_text().count("ran")
    # run() returns only once the next prompt has rendered, so the hook has had its chance.
    run(child, PROMPT, "true")
    assert marker.read_text().count("ran") > before
