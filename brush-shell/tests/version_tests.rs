//! Integration tests for brush shell
//!
//! Most CLI tests have been moved to YAML format in tests/cases/brush/cli.yaml.
//! This file contains tests that require dynamic value comparison.

// For now, only compile this for Unix-like platforms (Linux, macOS).
#![cfg(unix)]
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
