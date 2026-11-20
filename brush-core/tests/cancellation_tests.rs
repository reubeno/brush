//! Tests that cancelling a command partway through -- dropping its future, as a front end
//! does when the user hits Ctrl-C during completion -- leaves the shell as it was before
//! the command started.

#![cfg(unix)]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use std::time::Duration;

use anyhow::Result;
use brush_core::traps::TrapSignal;

/// A command that runs until it's cancelled.
const BLOCK: &str = "/bin/sleep 30";

async fn new_shell() -> Result<brush_core::Shell> {
    Ok(brush_core::Shell::builder()
        .do_not_inherit_env(true)
        .skip_well_known_vars(true)
        // Don't leave `sleep` running once a test cancels it.
        .kill_external_commands_on_drop(true)
        .build()
        .await?)
}

/// Waits on `run` until it's had time to block, then cancels it.
async fn cancel<T>(run: impl Future<Output = T>) {
    let finished = tokio::time::timeout(Duration::from_millis(250), run).await;
    assert!(
        finished.is_err(),
        "the command finished before it was cancelled"
    );
}

async fn run_and_cancel(shell: &mut brush_core::Shell, script: &str) {
    let params = shell.default_exec_params();
    cancel(shell.run_string(script, &brush_core::SourceInfo::default(), &params)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelled_function_is_left() -> Result<()> {
    let mut shell = new_shell().await?;

    run_and_cancel(&mut shell, &format!("f() {{ g; }}; g() {{ {BLOCK}; }}; f")).await;

    assert!(!shell.in_function());
    assert_eq!(shell.call_stack().function_call_depth(), 0);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelled_command_drops_its_assignments() -> Result<()> {
    let mut shell = new_shell().await?;

    run_and_cancel(&mut shell, &format!("f() {{ {BLOCK}; }}; FOO=1 f")).await;

    assert!(shell.env_str("FOO").is_none());

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelled_sourced_script_is_left() -> Result<()> {
    let mut shell = new_shell().await?;
    let script = tempfile::NamedTempFile::new()?;
    std::fs::write(script.path(), BLOCK)?;

    let params = shell.default_exec_params();
    cancel(shell.source_script(script.path(), std::iter::empty::<String>(), &params)).await;

    assert!(!shell.in_sourced_script());
    assert_eq!(shell.call_stack().script_source_depth(), 0);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelled_trap_handler_is_left() -> Result<()> {
    let mut shell = new_shell().await?;
    shell.traps_mut().register_handler(
        TrapSignal::Debug,
        format!("/bin/false; {BLOCK}"),
        brush_core::SourceInfo::default(),
    );
    shell.set_last_exit_status(7);

    run_and_cancel(&mut shell, "true").await;

    assert!(!shell.call_stack().is_trap_signal_active(TrapSignal::Debug));
    assert!(shell.call_stack().current_frame().is_none());
    assert_eq!(shell.last_exit_status(), 7);

    Ok(())
}
