//! Integration tests for brush shell
//!
//! Most CLI tests have been moved to YAML format in tests/cases/brush/cli.yaml.
//! This file contains tests that require dynamic value comparison.

// For now, only compile this for Unix-like platforms (Linux, macOS).
#![cfg(unix)]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Context;

#[test]
fn get_version_variables() -> anyhow::Result<()> {
    let shell_path = assert_cmd::cargo::cargo_bin!("brush");
    let brush_ver_str = get_variable(shell_path, /* shell_is_brush */ true, "BRUSH_VERSION")?;
    let bash_ver_str = get_variable(shell_path, /* shell_is_brush */ false, "BASH_VERSION")?;

    assert_eq!(brush_ver_str, env!("CARGO_PKG_VERSION"));
    assert_ne!(
        brush_ver_str, bash_ver_str,
        "Should differ for scripting use-case"
    );

    Ok(())
}

fn get_variable(
    shell_path: &std::path::Path,
    shell_is_brush: bool,
    var: &str,
) -> anyhow::Result<String> {
    let mut cmd = std::process::Command::new(shell_path);

    if shell_is_brush {
        cmd.arg("--no-config");
    }

    let output = cmd
        .arg("--norc")
        .arg("--noprofile")
        .arg("-c")
        .arg(format!("echo -n ${{{var}}}"))
        .output()
        .with_context(|| format!("failed to retrieve {var}"))?
        .stdout;
    Ok(String::from_utf8(output)?)
}

/// A shell started in a directory that no longer exists can't tell where it is. Like bash,
/// it says so but still runs: children start in the same (deleted) directory, and `cd` to
/// an absolute path recovers.
#[test]
fn starting_in_a_deleted_directory_warns_and_runs() -> anyhow::Result<()> {
    let shell_path = assert_cmd::cargo::cargo_bin!("brush");
    let scratch = tempfile::tempdir()?;

    // `sh` removes its own working directory, then becomes brush.
    let output = std::process::Command::new("sh")
        .current_dir(scratch.path())
        .env_remove("PWD")
        .arg("-c")
        .arg(r#"mkdir gone && cd gone && rmdir ../gone && exec "$0" --norc --noprofile --no-config -c "$1""#)
        .arg(shell_path)
        .arg("echo ran; pwd; echo \"pwd rc=$?\"; ls; echo \"ls rc=$?\"; cd /; pwd")
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "{stdout}{stderr}");
    assert_eq!(stdout, "ran\npwd rc=1\nls rc=0\n/\n", "{stderr}");
    assert!(stderr.contains("current directory"), "{stderr}");

    Ok(())
}

/// A relative `--working-dir` is resolved against the process's working directory, and like
/// any working directory, has no `.` or `..` in it.
#[test]
fn relative_working_dir_is_normalized() -> anyhow::Result<()> {
    let shell_path = assert_cmd::cargo::cargo_bin!("brush");
    let scratch = tempfile::tempdir()?;
    std::fs::create_dir(scratch.path().join("start"))?;
    std::fs::create_dir(scratch.path().join("wd"))?;

    let output = std::process::Command::new(shell_path)
        .current_dir(scratch.path().join("start"))
        .args(["--norc", "--noprofile", "--no-config"])
        .args(["--working-dir", "./../wd", "-c", "pwd"])
        .output()?;

    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        scratch.path().canonicalize()?.join("wd").to_string_lossy()
    );

    Ok(())
}

/// `--working-dir` must name an existing directory; brush refuses to start anywhere else.
#[test]
fn working_dir_must_be_an_existing_directory() -> anyhow::Result<()> {
    let shell_path = assert_cmd::cargo::cargo_bin!("brush");
    let scratch = tempfile::tempdir()?;
    std::fs::write(scratch.path().join("file"), "")?;

    for dir in ["missing", "file"] {
        let output = std::process::Command::new(shell_path)
            .current_dir(scratch.path())
            .args(["--norc", "--noprofile", "--no-config"])
            .args(["--working-dir", dir, "-c", "echo ran"])
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(!output.status.success(), "{dir}: {stdout}");
        assert!(stdout.is_empty(), "{dir}: {stdout}");
        assert!(stderr.contains(dir), "{dir}: {stderr}");
    }

    Ok(())
}

/// `--xtrace-file` applies to a shell loaded with `--load`, like any other.
#[cfg(all(feature = "experimental-load", feature = "experimental-builtins"))]
#[test]
fn xtrace_file_applies_to_a_loaded_shell() -> anyhow::Result<()> {
    let shell_path = assert_cmd::cargo::cargo_bin!("brush");
    let scratch = tempfile::tempdir()?;

    let saved = std::process::Command::new(shell_path)
        .current_dir(scratch.path())
        .args(["--norc", "--noprofile", "--no-config", "-c", "save"])
        .output()?;
    std::fs::write(scratch.path().join("snap.json"), saved.stdout)?;

    let output = std::process::Command::new(shell_path)
        .current_dir(scratch.path())
        .args(["--norc", "--noprofile", "--no-config"])
        .args(["--load", "snap.json", "--xtrace-file", "trace.log"])
        .args(["-c", "set -x; true"])
        .output()?;

    let trace = std::fs::read_to_string(scratch.path().join("trace.log"))?;
    assert!(trace.contains("true"), "{trace:?} {output:?}");

    Ok(())
}

/// `exec` runs a command by its path even when that path isn't UTF-8. (Not on macOS, whose
/// filesystems refuse names that aren't UTF-8.)
#[cfg(all(unix, not(target_vendor = "apple")))]
#[test]
fn exec_runs_a_command_in_a_directory_whose_name_is_not_utf8() -> anyhow::Result<()> {
    use std::os::unix::{ffi::OsStrExt as _, fs::PermissionsExt as _};

    let shell_path = assert_cmd::cargo::cargo_bin!("brush");
    let scratch = tempfile::tempdir()?;
    let dir = scratch.path().join(std::ffi::OsStr::from_bytes(b"dir\xff"));
    std::fs::create_dir(&dir)?;
    let prog = dir.join("prog");
    std::fs::write(&prog, "#!/bin/sh\necho ran\n")?;
    std::fs::set_permissions(&prog, std::fs::Permissions::from_mode(0o755))?;

    let output = std::process::Command::new(shell_path)
        .current_dir(&dir)
        .args(["--norc", "--noprofile", "--no-config", "-c", "exec ./prog"])
        .output()?;

    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "ran\n",
        "{output:?}"
    );

    Ok(())
}

/// An error about a file named on the command line names it the way the user did.
#[test]
fn command_line_paths_are_reported_as_given() -> anyhow::Result<()> {
    let shell_path = assert_cmd::cargo::cargo_bin!("brush");
    let scratch = tempfile::tempdir()?;

    for (args, expected) in [
        (["--config", "nope.toml"], "from nope.toml:"),
        (["--xtrace-file", "nodir/x.log"], "'nodir/x.log'"),
    ] {
        let output = std::process::Command::new(shell_path)
            .current_dir(scratch.path())
            .args(["--norc", "--noprofile"])
            .args(args)
            .args(["-c", "true"])
            .output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(stderr.contains(expected), "{args:?}: {stderr}");
    }

    Ok(())
}
