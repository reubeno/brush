"""Selected upstream mise bash activation tests, run against the shell under test.

Only tests that need no tool downloads are run. Each one is an upstream shell script sourced
alongside upstream's own `assert.sh`, so the assertions are mise's, not ours.
"""

import os
import subprocess

import pytest


UPSTREAM = "/mise/e2e"


@pytest.fixture
def run_upstream(tmp_path):
    def run(test):
        root = tmp_path / "root"
        home = tmp_path / "home"
        for path in (root, *(home / part for part in (
            ".config/mise", ".local/share/mise", ".local/state/mise", ".cache/mise"
        ))):
            path.mkdir(parents=True)
        shell = os.environ["MISE_TEST_SHELL"]
        result = subprocess.run(
            [
                shell, "--noprofile", "--norc", "-euo", "pipefail", "-c",
                f'cd "$1" && source {UPSTREAM}/assert.sh && source "$2"',
                "--", str(root), f"{UPSTREAM}/{test}",
            ],
            env=os.environ | {
                "HOME": str(home),
                "MISE_CACHE_DIR": str(home / ".cache/mise"),
                "MISE_CONFIG_DIR": str(home / ".config/mise"),
                "MISE_DATA_DIR": str(home / ".local/share/mise"),
                "MISE_EXPERIMENTAL": "1",
                "MISE_STATE_DIR": str(home / ".local/state/mise"),
                "MISE_TRUSTED_CONFIG_PATHS": str(root),
                "MISE_YES": "1",
                # The shim makes upstream's hard-coded `bash` the shell under test.
                "PATH": f"/e2e/bin:{os.environ['PATH']}",
                "SHELL": shell,
                "TEST_DIR": f"{UPSTREAM}/{os.path.dirname(test)}",
                "TEST_NAME": test,
                "TEST_ROOT": UPSTREAM,
            },
            capture_output=True,
            text=True,
            check=False,
        )
        assert result.returncode == 0, f"{result.stdout}{result.stderr}"

    return run


def test_bash_chpwd_hook_tolerates_nounset(run_upstream):
    run_upstream("env/test_bash_chpwd_functions_nounset")


def test_bash_prompt_hook_runs_consecutively(run_upstream):
    run_upstream("env/test_bash_consecutive_prompt_runs")


def test_bash_deactivate_clears_the_chpwd_hook(run_upstream):
    run_upstream("env/test_bash_deactivate_clears_chpwd_hook")


def test_bash_activation_skips_only_the_first_prompt_hook(run_upstream):
    run_upstream("env/test_bash_first_prompt_after_activate")


def test_bash_no_hook_env_leaves_the_environment_untouched(run_upstream):
    run_upstream("env/test_bash_no_hook_env")


def test_bash_activation_exports_the_mise_executable(run_upstream):
    run_upstream("cli/test_activate_export")


def test_bash_hook_env_preserves_path_reordering(run_upstream):
    run_upstream("env/test_path_reorder_after_activate")


def test_bash_hook_env_manages_shell_aliases(run_upstream):
    run_upstream("cli/test_shell_alias")
