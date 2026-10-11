//! Tests that a foreground external command that Ctrl-C ended stops the rest of the
//! running commands on Windows, the way bash stops when SIGINT ends a foreground child.
//!
//! A process that Windows ends because of Ctrl-C exits with `STATUS_CONTROL_C_EXIT`
//! (`0xC000013A`). The tests stand in for a real Ctrl-C with `cmd /c "exit -1073741510"`,
//! which exits with that same status, so no console event has to be sent.
//!
//! These live here rather than in `brush-core` because they need the shell's builtins
//! (`eval`, `source`, `set`, `trap`, `read`, `wait`, ...).
//!
//! Each script records what ran by assigning a variable, which the test then looks up.

#![cfg(windows)]
#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use std::io::Write as _;

use anyhow::Result;
use brush_builtins::ShellBuilderExt as _;

/// A command that exits the way Ctrl-C ends a process on Windows. `$cmd` is set by
/// [`new_shell`].
const CTRL_C_EXIT: &str = r#""$cmd" /c "exit -1073741510""#;

/// A command that fails without Ctrl-C.
const ORDINARY_FAILURE: &str = r#""$cmd" /c "exit 1""#;

/// A command that succeeds.
const SUCCESS: &str = r#""$cmd" /c "exit 0""#;

/// Builds a shell with the builtins, and with `$cmd` naming `cmd` by absolute path: the
/// shell doesn't inherit `PATH`.
async fn new_shell() -> Result<brush_core::Shell> {
    let mut shell = brush_core::Shell::builder()
        .do_not_inherit_env(true)
        .skip_well_known_vars(true)
        .default_builtins(brush_builtins::BuiltinSet::BashMode)
        .build()
        .await?;

    let comspec =
        std::env::var("ComSpec").unwrap_or_else(|_| r"C:\Windows\System32\cmd.exe".into());
    run(&mut shell, &std::format!("cmd='{comspec}'")).await?;

    Ok(shell)
}

/// Runs a script the way a top-level command line runs: with fresh execution parameters.
async fn run(shell: &mut brush_core::Shell, script: &str) -> Result<brush_core::ExecutionResult> {
    let params = shell.default_exec_params();
    Ok(shell
        .run_string(script, &brush_core::SourceInfo::default(), &params)
        .await?)
}

fn is_set(shell: &brush_core::Shell, name: &str) -> bool {
    shell.env_str(name).is_some()
}

#[tokio::test]
async fn ctrl_c_exit_leaves_while_loop_and_stops_what_follows() -> Result<()> {
    let mut shell = new_shell().await?;

    run(
        &mut shell,
        &std::format!("while :; do {CTRL_C_EXIT}; done; after=1"),
    )
    .await?;

    assert!(!is_set(&shell, "after"));
    assert_eq!(shell.last_exit_status(), 130);

    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_leaves_until_loop_from_its_condition() -> Result<()> {
    let mut shell = new_shell().await?;

    run(
        &mut shell,
        &std::format!("until {CTRL_C_EXIT}; do body=1; done"),
    )
    .await?;

    // The loop reports the interrupted condition, not the status of an earlier body.
    assert!(!is_set(&shell, "body"));
    assert_eq!(shell.last_exit_status(), 130);

    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_leaves_for_loop() -> Result<()> {
    let mut shell = new_shell().await?;

    run(
        &mut shell,
        &std::format!("for i in 1 2; do last=$i; {CTRL_C_EXIT}; done"),
    )
    .await?;

    assert_eq!(shell.env_str("last").as_deref(), Some("1"));
    assert_eq!(shell.last_exit_status(), 130);

    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_leaves_arithmetic_for_loop() -> Result<()> {
    let mut shell = new_shell().await?;

    run(
        &mut shell,
        &std::format!("for ((i = 0; i < 2; i++)); do last=$i; {CTRL_C_EXIT}; done"),
    )
    .await?;

    assert_eq!(shell.env_str("last").as_deref(), Some("0"));
    assert_eq!(shell.last_exit_status(), 130);

    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_stops_the_next_command_in_a_list() -> Result<()> {
    let mut shell = new_shell().await?;

    run(&mut shell, &std::format!("{CTRL_C_EXIT}; after=1")).await?;

    assert!(!is_set(&shell, "after"));

    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_stops_and_or_list_and_the_next_command() -> Result<()> {
    let mut shell = new_shell().await?;

    run(&mut shell, &std::format!("{CTRL_C_EXIT} || a=1; b=1")).await?;

    assert!(!is_set(&shell, "a"));
    assert!(!is_set(&shell, "b"));

    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_in_function_stops_the_caller() -> Result<()> {
    let mut shell = new_shell().await?;

    run(
        &mut shell,
        &std::format!("f() {{ {CTRL_C_EXIT}; inner=1; }}; f; after=1"),
    )
    .await?;

    assert!(!is_set(&shell, "inner"));
    assert!(!is_set(&shell, "after"));

    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_in_eval_stops_the_caller() -> Result<()> {
    let mut shell = new_shell().await?;

    run(&mut shell, &std::format!("eval '{CTRL_C_EXIT}'; after=1")).await?;

    assert!(!is_set(&shell, "after"));

    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_in_sourced_file_stops_the_file_and_the_caller() -> Result<()> {
    let mut shell = new_shell().await?;

    let mut script = tempfile::NamedTempFile::new()?;
    writeln!(script, "{CTRL_C_EXIT}")?;
    writeln!(script, "rest=1")?;
    script.flush()?;

    run(
        &mut shell,
        &std::format!("source '{}'; after=1", script.path().display()),
    )
    .await?;

    assert!(!is_set(&shell, "rest"));
    assert!(!is_set(&shell, "after"));

    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_in_subshell_stops_the_parent() -> Result<()> {
    let mut shell = new_shell().await?;

    run(&mut shell, &std::format!("({CTRL_C_EXIT}); after=1")).await?;

    assert!(!is_set(&shell, "after"));

    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_in_command_substitution_stops_the_parent() -> Result<()> {
    let mut shell = new_shell().await?;

    run(&mut shell, &std::format!("v=$({CTRL_C_EXIT}); after=1")).await?;

    assert!(!is_set(&shell, "after"));

    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_in_pipeline_stops_what_follows() -> Result<()> {
    let mut shell = new_shell().await?;

    run(
        &mut shell,
        &std::format!("{CTRL_C_EXIT} | {SUCCESS}; after=1"),
    )
    .await?;

    assert!(!is_set(&shell, "after"));

    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_in_compound_stage_skips_later_stages() -> Result<()> {
    let mut shell = new_shell().await?;

    // The brace group runs to completion before the next stage would be started. Had the
    // last stage run, the pipeline would report its success.
    run(
        &mut shell,
        &std::format!("{{ {CTRL_C_EXIT}; }} | {SUCCESS}"),
    )
    .await?;

    assert_eq!(shell.last_exit_status(), 130);

    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_in_if_condition_runs_no_branch() -> Result<()> {
    let mut shell = new_shell().await?;

    run(
        &mut shell,
        &std::format!("if {CTRL_C_EXIT}; then t=1; else e=1; fi"),
    )
    .await?;

    assert!(!is_set(&shell, "t"));
    assert!(!is_set(&shell, "e"));

    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_in_case_branch_does_not_fall_through() -> Result<()> {
    let mut shell = new_shell().await?;

    run(
        &mut shell,
        &std::format!("case a in a) {CTRL_C_EXIT} ;& b) b=1 ;; esac"),
    )
    .await?;

    assert!(!is_set(&shell, "b"));

    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_under_errexit_does_not_exit_the_shell() -> Result<()> {
    let mut shell = new_shell().await?;

    let result = run(&mut shell, &std::format!("set -e; {CTRL_C_EXIT}")).await?;

    assert!(!result.is_exit());

    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_does_not_fire_err_trap() -> Result<()> {
    let mut shell = new_shell().await?;

    run(&mut shell, &std::format!("trap 'err=1' ERR; {CTRL_C_EXIT}")).await?;

    assert!(!is_set(&shell, "err"));

    Ok(())
}

// `read` blocks the thread it runs on while the process substitution is still running in
// another task.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ctrl_c_exit_in_process_substitution_stops_the_reader() -> Result<()> {
    let mut shell = new_shell().await?;

    run(
        &mut shell,
        &std::format!("while read l; do :; done < <({CTRL_C_EXIT}); after=1"),
    )
    .await?;

    assert!(!is_set(&shell, "after"));

    Ok(())
}

#[tokio::test]
async fn ctrl_c_exit_in_background_list_does_not_stop_the_foreground() -> Result<()> {
    let mut shell = new_shell().await?;

    // `wait` makes sure the background command has ended before `after=1` runs.
    run(&mut shell, &std::format!("{CTRL_C_EXIT} & wait; after=1")).await?;

    assert!(is_set(&shell, "after"));

    Ok(())
}

#[tokio::test]
async fn ordinary_failure_does_not_stop_what_follows() -> Result<()> {
    let mut shell = new_shell().await?;

    run(&mut shell, &std::format!("{ORDINARY_FAILURE}; after=1")).await?;

    assert!(is_set(&shell, "after"));

    Ok(())
}

#[tokio::test]
async fn each_top_level_run_starts_uninterrupted() -> Result<()> {
    let mut shell = new_shell().await?;

    run(&mut shell, CTRL_C_EXIT).await?;
    run(&mut shell, "after=1").await?;

    assert!(is_set(&shell, "after"));

    Ok(())
}
