import os
import subprocess

import pytest
from pty_shell import run, spawn
from rcfile import write_rc


OMG = "/opt/oh-my-git/prompt.sh"
# The rcfile's own PS1. oh-my-git replaces it at every prompt with one of the two below, so seeing
# either proves its PROMPT_COMMAND reached build_prompt; it comes back only when a repo opts out.
ORIGINAL_PROMPT = "omg-original> "
NO_REPO_PROMPT = "omg-norepo> "
REPO_PROMPT = "omg-repo> "
BRANCH = "e2e-branch"
# oh-my-git's own symbols are private-use glyphs from a patched font; words are easier to match.
SETTINGS = (
    f"omg_ungit_prompt='{NO_REPO_PROMPT}'",
    f"omg_second_line='{REPO_PROMPT}'",
    "omg_has_untracked_files_symbol=UNTRACKED",
    "omg_has_modifications_symbol=MODIFIED",
)


def git(repo, *args):
    subprocess.run(
        ["git", "-c", "user.name=e2e", "-c", "user.email=e2e@example.com", "-C", str(repo), *args],
        check=True,
        capture_output=True,
    )


@pytest.fixture
def home(tmp_path):
    home = tmp_path / "home"
    repo = home / "repo"
    repo.mkdir(parents=True)
    git(repo, "init", "-q", "-b", BRANCH)
    (repo / "tracked").write_text("one\n")
    git(repo, "add", "tracked")
    git(repo, "commit", "-q", "-m", "init")
    return home


@pytest.fixture
def start(tmp_path, home):
    children = []

    # Each test starts its own shell, so one whose rcfile differs (#471's) can still share this.
    def start(*lines, prompt=NO_REPO_PROMPT):
        rc = write_rc(tmp_path / "rc", ORIGINAL_PROMPT.rstrip(), *lines, 'cd "$HOME"')
        child = spawn(
            os.environ["OMG_TEST_SHELL"],
            ["--noprofile", "--rcfile", str(rc)],
            env=os.environ | {"HOME": str(home), "TERM": "dumb"},
            prompt=prompt,
        )
        children.append(child)
        return child

    yield start
    for child in children:
        child.close(force=True)


@pytest.fixture
def shell(start):
    return start(*SETTINGS, f"source {OMG}")


def test_prompt_replaces_ps1_outside_a_repo(shell):
    assert "function\r\n" in run(shell, NO_REPO_PROMPT, "type -t build_prompt")


def test_sourced_from_a_function_finds_base_sh(start):
    # #471: prompt.sh finds base.sh (and build_prompt) through BASH_SOURCE[0], which must name
    # prompt.sh itself rather than the rcfile whose function sourced it.
    child = start(*SETTINGS, f"load_omg() {{ source {OMG}; }}", "load_omg")
    assert "function\r\n" in run(child, NO_REPO_PROMPT, "type -t build_prompt")


def test_prompt_shows_the_branch_inside_a_repo(shell):
    output = run(shell, REPO_PROMPT, "cd repo")
    assert f"({BRANCH})" in output
    assert "UNTRACKED" not in output and "MODIFIED" not in output


def test_prompt_flags_untracked_and_modified_files(shell, home):
    run(shell, REPO_PROMPT, "cd repo")
    (home / "repo" / "tracked").write_text("two\n")
    (home / "repo" / "new").touch()
    output = run(shell, REPO_PROMPT, "true")
    assert "UNTRACKED" in output and "MODIFIED" in output


def test_repo_can_turn_the_prompt_off(shell, home):
    # build_prompt `exit`s from its command substitution here, after echoing the original PS1.
    git(home / "repo", "config", "oh-my-git.enabled", "false")
    run(shell, ORIGINAL_PROMPT, "cd repo")


def test_prompt_callback_output_is_included(shell):
    run(shell, "CALLBACK" + NO_REPO_PROMPT, "omg_prompt_callback() { echo CALLBACK; }")


def test_command_number_in_the_prompt(start):
    # #471's reporter keeps `[\#]` in the prompt oh-my-git falls back to outside a repo.
    child = start("omg_ungit_prompt='[\\#]omg-norepo> '", f"source {OMG}", prompt="[1]omg-norepo> ")
    run(child, "[2]omg-norepo> ", "true")
