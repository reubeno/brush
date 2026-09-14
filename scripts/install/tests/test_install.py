"""Tests for scripts/install/install.sh, run against real GitHub releases (requires network access).

Each test pipes the installer into `sh -s --` (and `bash -s --`), just like `curl ... | sh` would. Failure modes
are simulated with shims for curl, gh, tar, and getconf placed ahead of the real tools on PATH.

Usage: python3 -m pytest scripts/install/tests
(Set GH_TOKEN, e.g. to `$(gh auth token)`, to also verify a real build attestation.)
"""

import os
import platform
import shutil
import subprocess
from pathlib import Path

import pytest


INSTALLER = Path(__file__).resolve().parent.parent / "install.sh"
# Deliberately not the latest release, so tests can tell pinned installs from latest ones.
VERSION = "0.3.0"

# Shim name -> (tool it replaces, bash body). $REAL_CURL and $REAL_TAR name the real tools.
SHIMS = {
    "bad-checksum": (
        "curl",
        """
        "${REAL_CURL}" "$@" || exit
        prev=""
        for arg; do
            if [[ ${prev} == -o && ${arg} == *.sha256 ]]; then
                line="$(<"${arg}")"
                first=0 && [[ ${line} == 0* ]] && first=1
                echo "${first}${line:1}" >"${arg}"
            fi
            prev="${arg}"
        done
        """,
    ),
    "bad-attestation": (
        "gh",
        """
        [[ $* == *--help ]] && { echo "--source-ref"; exit 0; }
        echo "simulated attestation failure" >&2
        exit 1
        """,
    ),
    # gh exits 4 when it has no credentials.
    "gh-unauthenticated": (
        "gh",
        """
        [[ $* == *--help ]] && { echo "--source-ref"; exit 0; }
        echo "To get started with GitHub CLI, please run:  gh auth login" >&2
        exit 4
        """,
    ),
    # e.g. the gh shipped by Ubuntu 24.04 and Debian 12: no `attestation` subcommand at all.
    "gh-too-old": (
        "gh",
        """
        echo 'unknown command "attestation" for "gh"' >&2
        exit 1
        """,
    ),
    "broken-binary": (
        "tar",
        """
        "${REAL_TAR}" "$@" || exit
        while [[ $# -gt 0 ]]; do
            [[ $1 == -C ]] && printf '#!/bin/sh\\nexit 1\\n' >"$2/brush"
            shift
        done
        """,
    ),
    "old-glibc": ("getconf", 'echo "glibc 2.17"'),
}

GH_INSTALLED = shutil.which("gh") is not None

# The script is POSIX sh; test it under the system sh (dash on Debian/Ubuntu, busybox ash on
# Alpine) and under bash. On macOS, also under the ancient /bin/bash (3.2) if PATH has a newer one.
SHELLS = ["sh", "bash"]
if platform.system() == "Darwin" and Path(shutil.which("bash")).resolve() != Path("/bin/bash"):
    SHELLS.append("/bin/bash")


@pytest.fixture(params=SHELLS)
def install(request, tmp_path):
    """Runs the installer with the given args, shims, and environment overrides."""

    def run(*args, shims=(), **env_overrides):
        env = os.environ | {
            "REAL_CURL": shutil.which("curl"),
            "REAL_TAR": shutil.which("tar"),
        }
        for name in shims:
            tool, body = SHIMS[name]
            shim = tmp_path / "shims" / name / tool
            shim.parent.mkdir(parents=True, exist_ok=True)
            shim.write_text("#!/usr/bin/env bash\n" + body)
            shim.chmod(0o755)
            env["PATH"] = f"{shim.parent}{os.pathsep}{env['PATH']}"
        for key, value in env_overrides.items():  # None unsets the variable
            if value is None:
                env.pop(key, None)
            else:
                env[key] = str(value)

        return subprocess.run(
            [request.param, "-s", "--", *map(str, args)],
            input=INSTALLER.read_text(),
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            env=env,
        )

    return run


def assert_succeeded(result):
    assert result.returncode == 0, result.stdout
    # All output should be the installer's own; anything else means some tool misbehaved.
    assert all(line.startswith("brush-install: ") for line in result.stdout.splitlines()), (
        result.stdout
    )


def assert_failed(result, message):
    assert result.returncode != 0, result.stdout
    # Every failure should end with the installer's own error message.
    last_line = result.stdout.splitlines()[-1]
    assert last_line.startswith("brush-install: error: "), result.stdout
    assert message in last_line, result.stdout


def brush_version(path):
    return subprocess.run(
        [path, "--version"], capture_output=True, text=True, check=True
    ).stdout


def test_latest_release_to_default_dir(install, tmp_path):
    home = tmp_path / "home"
    # Without any other brush on PATH, so the note is about PATH rather than shadowing.
    path = os.pathsep.join(d for d in os.environ["PATH"].split(os.pathsep) if not (Path(d) / "brush").exists())
    result = install(HOME=home, PATH=path, XDG_BIN_HOME=None)
    assert_succeeded(result)

    bin_dir = home / ".local" / "bin"
    version = brush_version(bin_dir / "brush")
    assert version.startswith("brush ")
    assert not version.startswith(f"brush {VERSION} ")
    assert os.listdir(bin_dir) == ["brush"]
    assert f"note: {bin_dir} is not in your PATH" in result.stdout


def test_pinned_version_replaces_existing_binary(install, tmp_path):
    (tmp_path / "brush").write_text("old")
    result = install("--version", f"v{VERSION}", "--dir", tmp_path)
    assert_succeeded(result)
    assert brush_version(tmp_path / "brush").startswith(f"brush {VERSION} ")


def test_xdg_bin_home_is_default_dir(install, tmp_path):
    xdg_bin = tmp_path / "xdg-bin"
    result = install("--version", VERSION, HOME=tmp_path / "home", XDG_BIN_HOME=xdg_bin)
    assert_succeeded(result)
    assert os.listdir(xdg_bin) == ["brush"]


@pytest.mark.skipif(platform.system() != "Linux", reason="glibc detection is Linux-only")
def test_old_glibc_falls_back_to_musl(install, tmp_path):
    result = install("--version", VERSION, "--dir", tmp_path, shims=["old-glibc"])
    assert_succeeded(result)
    assert "-unknown-linux-musl.tar.gz" in result.stdout


@pytest.mark.skipif(not GH_INSTALLED, reason="gh is not installed")
# CI always provides GH_TOKEN, so never skip there: a missing token should fail, not skip.
@pytest.mark.skipif(not os.environ.get("GH_TOKEN") and not os.environ.get("CI"), reason="GH_TOKEN is not set")
def test_verifies_real_attestation(install, tmp_path):
    result = install("--version", VERSION, "--dir", tmp_path, "--require-attestation")
    assert_succeeded(result)
    assert "verified GitHub build attestation" in result.stdout


# Runs on the CI images that deliberately lack gh (see .github/workflows/install-script.yaml).
@pytest.mark.skipif(GH_INSTALLED, reason="gh is installed")
def test_missing_gh_warns_with_reason(install, tmp_path):
    result = install("--version", VERSION, "--dir", tmp_path)
    assert_succeeded(result)
    assert "note: GitHub CLI (gh) is not installed, so the build attestation wasn't checked" in result.stdout


def test_unauthenticated_gh_warns_with_reason(install, tmp_path):
    result = install("--version", VERSION, "--dir", tmp_path, shims=["gh-unauthenticated"])
    assert_succeeded(result)
    assert (
        "note: GitHub CLI (gh) is not authenticated, so the build attestation wasn't checked"
        in result.stdout
    )


def test_too_old_gh_warns_with_reason(install, tmp_path):
    result = install("--version", VERSION, "--dir", tmp_path, shims=["gh-too-old"])
    assert_succeeded(result)
    assert (
        "note: GitHub CLI (gh) is too old (2.68 or newer is needed), so the build attestation wasn't checked"
        in result.stdout
    )


def test_notes_when_another_brush_comes_first_in_path(install, tmp_path):
    earlier, dest = tmp_path / "earlier", tmp_path / "dest"
    earlier.mkdir()
    (earlier / "brush").write_text("#!/bin/sh\n")
    (earlier / "brush").chmod(0o755)

    path = os.pathsep.join([str(earlier), str(dest), os.environ["PATH"]])
    result = install("--version", VERSION, "--dir", dest, PATH=path)
    assert_succeeded(result)
    assert f"note: running 'brush' will run {earlier / 'brush'}, not {dest / 'brush'}" in result.stdout


def test_no_path_note_when_dir_is_in_path_via_symlink(install, tmp_path):
    dest, link = tmp_path / "dest", tmp_path / "link"
    link.symlink_to(dest)

    path = os.pathsep.join([str(link), os.environ["PATH"]])
    result = install("--version", VERSION, "--dir", dest, PATH=path)
    assert_succeeded(result)
    # Only the attestation note is allowed (gh may be missing or unauthenticated here).
    assert not [line for line in result.stdout.splitlines() if "note:" in line and "attestation" not in line]


def test_unset_home_without_dir(install):
    assert_failed(install("--version", VERSION, HOME=None, XDG_BIN_HOME=None), "HOME is not set")


@pytest.mark.parametrize(
    "args, message",
    [
        (["--bogus"], "unknown option: --bogus"),
        (["--version"], "--version requires a value"),
        (["--dir"], "--dir requires a value"),
    ],
    ids=["unknown-option", "missing-version", "missing-dir"],
)
def test_bad_arguments(install, args, message):
    assert_failed(install(*args), message)


@pytest.mark.parametrize(
    "args, shims, message",
    [
        pytest.param(
            ["--version", "0.0.0"], [], "failed to download", id="nonexistent-version"
        ),
        pytest.param(
            ["--version", VERSION], ["bad-checksum"], "checksum mismatch", id="bad-checksum"
        ),
        pytest.param(
            ["--version", VERSION],
            ["bad-attestation"],
            "attestation verification failed",
            id="bad-attestation",
        ),
        pytest.param(
            ["--version", VERSION, "--require-attestation"],
            ["gh-unauthenticated"],
            "cannot verify build attestation: GitHub CLI (gh) is not authenticated",
            id="require-attestation-unauthenticated",
        ),
        pytest.param(
            ["--version", VERSION, "--require-attestation"],
            ["gh-too-old"],
            "cannot verify build attestation: GitHub CLI (gh) is too old",
            id="require-attestation-too-old",
        ),
        pytest.param(
            ["--version", VERSION, "--require-attestation"],
            [],
            "cannot verify build attestation: GitHub CLI (gh) is not installed",
            id="require-attestation-no-gh",
            # A real gh can't be hidden from PATH, so this only runs where gh is absent.
            marks=pytest.mark.skipif(GH_INSTALLED, reason="gh is installed"),
        ),
    ],
)
def test_failure_installs_nothing(install, tmp_path, args, shims, message):
    dest = tmp_path / "dest"
    assert_failed(install(*args, "--dir", dest, shims=shims), message)
    assert not dest.exists()


def test_unwritable_dir(install, tmp_path):
    # A path beneath a regular file can't be created, even by root (as in CI containers).
    (tmp_path / "file").write_text("")
    dest = tmp_path / "file" / "bin"
    assert_failed(install("--version", VERSION, "--dir", dest), f"cannot write to {dest}")


def test_broken_binary_keeps_existing_one(install, tmp_path):
    dest = tmp_path / "dest"
    dest.mkdir()
    (dest / "brush").write_text("old")
    result = install("--version", VERSION, "--dir", dest, shims=["broken-binary"])
    assert_failed(result, "build failed to run on this system")
    assert os.listdir(dest) == ["brush"]
    assert (dest / "brush").read_text() == "old"
