//! Tests for running builtins through their registrations, outside of a script.

#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use std::io::Read as _;

use anyhow::{Context as _, Result};
use brush_builtins::{BuiltinSet, ShellBuilderExt as _};
use brush_core::{CommandArg, ExecutionContext, openfiles::OpenFiles};

async fn new_shell() -> Result<brush_core::Shell> {
    Ok(brush_core::Shell::builder()
        .default_builtins(BuiltinSet::BashMode)
        .build()
        .await?)
}

#[tokio::test]
async fn a_builtin_runs_in_a_context_made_by_hand() -> Result<()> {
    let mut shell = new_shell().await?;
    let echo = shell.builtins().get("echo").context("no echo")?.clone();

    let (mut reader, writer) = std::io::pipe()?;
    let mut params = shell.default_exec_params();
    params.set_fd(OpenFiles::STDOUT_FD, writer.into());

    let context = ExecutionContext::new(&mut shell, "echo".into(), params);
    let args = ["echo", "hello", "world"]
        .map(|arg| CommandArg::String(arg.into()))
        .into();
    let result = echo.execute(context, args).await?;
    assert!(result.is_success());

    // The context, and with it the pipe's write end, is gone, so this reads
    // to the end of what echo wrote.
    let mut output = String::new();
    reader.read_to_string(&mut output)?;
    assert_eq!(output, "hello world\n");

    Ok(())
}

#[tokio::test]
async fn a_registration_reports_what_enable_changes() -> Result<()> {
    let mut shell = new_shell().await?;
    let echo = shell.builtins().get("echo").context("no echo")?;
    assert!(echo.is_enabled());
    assert!(!echo.is_special());
    let export = shell.builtins().get("export").context("no export")?;
    assert!(export.is_special());

    let params = shell.default_exec_params();
    let result = shell
        .run_string(
            "enable -n echo",
            &brush_core::SourceInfo::default(),
            &params,
        )
        .await?;
    assert!(result.is_success());
    let echo = shell.builtins().get("echo").context("no echo")?;
    assert!(!echo.is_enabled());

    // Enabling it through the registration shows in what `enable` lists.
    shell
        .builtins_mut()
        .get_mut("echo")
        .context("no echo")?
        .set_enabled(true);
    let result = shell
        .run_string(
            "listed=$(enable)",
            &brush_core::SourceInfo::default(),
            &params,
        )
        .await?;
    assert!(result.is_success());
    let listed = shell.env_str("listed").context("listed is unset")?;
    assert!(listed.lines().any(|line| line == "enable echo"));

    Ok(())
}
