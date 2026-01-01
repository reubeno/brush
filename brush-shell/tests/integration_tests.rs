//! Integration tests for brush shell

// For now, only compile this for Unix-like platforms (Linux, macOS).
#![cfg(unix)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Context;
use predicates::prelude::PredicateBooleanExt;

#[test]
fn get_version_variables() -> anyhow::Result<()> {
    let shell_path = assert_cmd::cargo::cargo_bin!("brush");
    let brush_ver_str = get_variable(shell_path, "BRUSH_VERSION")?;
    let bash_ver_str = get_variable(shell_path, "BASH_VERSION")?;

    assert_eq!(brush_ver_str, env!("CARGO_PKG_VERSION"));
    assert_ne!(
        brush_ver_str, bash_ver_str,
        "Should differ for scripting use-case"
    );

    Ok(())
}

#[test]
fn version_exit_code() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("brush");
    let assert = cmd.arg("--version").assert();
    assert
        .success()
        .stdout(predicates::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn help_exit_code() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("brush");
    let assert = cmd.arg("--help").assert();
    assert.success().stdout(predicates::str::is_empty().not());
}

#[test]
fn invalid_option_exit_code() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("brush");
    let assert = cmd.arg("--unknown-argument-here").assert();
    assert
        .failure()
        .stderr(predicates::str::contains("unexpected argument"));
}

fn get_variable(shell_path: &std::path::Path, var: &str) -> anyhow::Result<String> {
    let output = std::process::Command::new(shell_path)
        .arg("--norc")
        .arg("--noprofile")
        .arg("--no-config")
        .arg("-c")
        .arg(format!("echo -n ${{{var}}}"))
        .output()
        .with_context(|| format!("failed to retrieve {var}"))?
        .stdout;
    Ok(String::from_utf8(output)?)
}

// Config file tests

#[test]
fn no_config_flag_works() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("brush");
    let assert = cmd.arg("--no-config").arg("-c").arg("echo ok").assert();
    assert.success().stdout(predicates::str::contains("ok"));
}

#[test]
fn explicit_config_file_not_found_fails() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("brush");
    let assert = cmd
        .arg("--config")
        .arg("/nonexistent/path/to/config.toml")
        .arg("-c")
        .arg("echo should_not_run")
        .assert();
    assert.failure().stderr(predicates::str::contains("config"));
}

#[test]
fn explicit_config_file_valid() -> anyhow::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let config_path = temp_dir.path().join("config.toml");
    std::fs::write(
        &config_path,
        r"
[ui]
syntax-highlighting = false

[experimental]
zsh-hooks = false
",
    )?;

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("brush");
    let assert = cmd
        .arg("--config")
        .arg(&config_path)
        .arg("-c")
        .arg("echo config_loaded")
        .assert();
    assert
        .success()
        .stdout(predicates::str::contains("config_loaded"));
    Ok(())
}

#[test]
fn explicit_config_file_with_unknown_fields() -> anyhow::Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let config_path = temp_dir.path().join("config.toml");
    std::fs::write(
        &config_path,
        r#"
[ui]
syntax-highlighting = true
future-setting = "ignored"

[unknown-section]
foo = "bar"
"#,
    )?;

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("brush");
    let assert = cmd
        .arg("--config")
        .arg(&config_path)
        .arg("-c")
        .arg("echo forward_compat")
        .assert();
    // Should succeed despite unknown fields
    assert
        .success()
        .stdout(predicates::str::contains("forward_compat"));
    Ok(())
}
