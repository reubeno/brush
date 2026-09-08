import os
import resource

import pytest
from pty_shell import run, spawn
from rcfile import write_rc


PROMPT = "nvm-test> "

# The versions the fixture fabricates, and the one it puts on PATH and makes the default. These
# are arbitrary: nvm reads them from the directory names the fixture creates, and the fake `node`
# in each just echoes its own name, so nothing here tracks real node releases.
VERSIONS = ("v20.11.0", "v22.14.0")
DEFAULT = VERSIONS[-1]

# nvm's LTS codenames, one alias file each. Only their number matters: `nvm ls` resolves every
# alias, which is where #1173 leaked a file descriptor apiece. Each points at a version the
# fixture does *not* create, as an older LTS line does on a real installation, and those versions
# are generated on nvm's even-major cadence rather than copied from a table that would have to be
# kept current.
LTS_ALIASES = (
    "argon", "boron", "carbon", "dubnium", "erbium", "fermium",
    "gallium", "hydrogen", "iron", "jod", "krypton",
)
UNINSTALLED = [f"v{4 + 2 * index}.0.0" for index in range(len(LTS_ALIASES))]


@pytest.fixture
def shell(tmp_path):
    home = tmp_path / "home"
    nvm_dir = tmp_path / "nvm"
    home.mkdir()
    for version in VERSIONS:
        node = nvm_dir / "versions" / "node" / version / "bin" / "node"
        node.parent.mkdir(parents=True)
        node.write_text(f"#!/bin/sh\necho {version}\n")
        node.chmod(0o755)
    aliases = nvm_dir / "alias"
    lts_aliases = aliases / "lts"
    lts_aliases.mkdir(parents=True)
    (aliases / "default").write_text(f"{DEFAULT}\n")
    for name, version in zip(LTS_ALIASES, UNINSTALLED):
        (lts_aliases / name).write_text(f"{version}\n")

    rc = write_rc(
        tmp_path / "rc",
        PROMPT.rstrip(),
        "export NVM_NO_COLORS=--no-colors",
        ". /nvm/nvm.sh",
    )
    env = os.environ | {
        "HOME": str(home),
        "NVM_DIR": str(nvm_dir),
        "PATH": f"{nvm_dir}/versions/node/{DEFAULT}/bin:{os.environ['PATH']}",
        "TERM": "dumb",
    }
    soft_limit, hard_limit = resource.getrlimit(resource.RLIMIT_NOFILE)
    child_limit = 128 if hard_limit == resource.RLIM_INFINITY else min(128, hard_limit)
    resource.setrlimit(resource.RLIMIT_NOFILE, (child_limit, hard_limit))
    try:
        child = spawn(
            os.environ["NVM_TEST_SHELL"],
            ["--noprofile", "--rcfile", str(rc)],
            env=env,
            prompt=PROMPT,
        )
    finally:
        resource.setrlimit(resource.RLIMIT_NOFILE, (soft_limit, hard_limit))
    yield child
    child.close(force=True)


def test_interactive_nvm_ls(shell):
    output = run(shell, PROMPT, "nvm ls")
    for version in VERSIONS:
        assert version in output
    assert "Too many open files" not in output
    assert "nvm_print_alias_" not in output
