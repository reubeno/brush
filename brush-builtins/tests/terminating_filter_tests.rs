//! A static command filter can stop a running interpreter without having its
//! error converted into a recoverable per-command exit status.
#![cfg(test)]
#![cfg(all(
    test,
    feature = "builtin.colon",
    feature = "builtin.true",
    feature = "builtin.false",
    feature = "builtin.dot",
    feature = "builtin.eval",
    feature = "builtin.trap"
))]
#![allow(clippy::panic_in_result_fn)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use anyhow::Result;
use brush_core::extensions::{DefaultErrorFormatter, ShellExtensions, ShellExtensionsImpl};
use brush_core::filter::{CmdExecFilter, PreFilterResult, SimpleCmdOutput, SimpleCmdParams};
use brush_core::{Error, ExecutionControlFlow, ExecutionResult, ExecutionSpawnResult, Shell};

#[derive(Clone, Default)]
struct BudgetFilter {
    body_calls: Arc<AtomicUsize>,
    continued: Arc<AtomicBool>,
    fallback: Arc<AtomicBool>,
    terminating: bool,
    allowance: usize,
    broken_pipe: bool,
}

impl CmdExecFilter for BudgetFilter {
    #[allow(
        clippy::unused_async_trait_impl,
        reason = "the filter must count commands only when its future is polled"
    )]
    async fn pre_simple_cmd<'a, SE: ShellExtensions>(
        &self,
        params: SimpleCmdParams<'a, SE>,
    ) -> PreFilterResult<SimpleCmdParams<'a, SE>, SimpleCmdOutput> {
        if params.command_name() == "false" {
            self.continued.store(true, Ordering::Relaxed);
        }
        if params.command_name() != ":" {
            return PreFilterResult::Continue(params);
        }
        let calls = self.body_calls.fetch_add(1, Ordering::Relaxed);
        // Keep a broken implementation bounded too. This emergency exit is
        // reached only if the interpreter wrongly continued after the marked
        // error; tests assert it was never needed. A timer within a CPU-bound
        // builtin loop would be starved and could leak a red-test worker.
        if calls > self.allowance + 8 {
            self.fallback.store(true, Ordering::Relaxed);
            return PreFilterResult::Return(Ok(ExecutionSpawnResult::Completed(ExecutionResult {
                next_control_flow: ExecutionControlFlow::ExitShell,
                ..ExecutionResult::general_error()
            })));
        }
        if calls < self.allowance {
            return PreFilterResult::Continue(params);
        }
        let kind = if self.broken_pipe {
            std::io::ErrorKind::BrokenPipe
        } else {
            std::io::ErrorKind::Other
        };
        let error = Error::from(std::io::Error::new(kind, "run budget exhausted"));
        PreFilterResult::Return(Err(if self.terminating {
            error.into_terminating()
        } else {
            error
        }))
    }
}

type BudgetExtensions = ShellExtensionsImpl<DefaultErrorFormatter, BudgetFilter>;

async fn shell(filter: BudgetFilter, interactive: bool) -> Result<Shell<BudgetExtensions>> {
    Ok(Shell::builder_with_extensions::<BudgetExtensions>()
        .cmd_exec_filter(filter)
        .builtins(brush_builtins::default_builtins::<BudgetExtensions>(
            brush_builtins::BuiltinSet::BashMode,
        ))
        .interactive(interactive)
        .do_not_inherit_env(true)
        .skip_well_known_vars(true)
        .build()
        .await?)
}

async fn assert_terminates(script: &str, interactive: bool) -> Result<()> {
    let filter = BudgetFilter {
        terminating: true,
        allowance: 2,
        ..Default::default()
    };
    let mut shell = shell(filter.clone(), interactive).await?;
    let result = shell
        .run_string(
            script,
            &brush_core::SourceInfo::default(),
            &shell.default_exec_params(),
        )
        .await;
    assert!(
        matches!(result, Err(ref error) if error.is_terminating()),
        "terminating filter error must escape the run: error={:?}, exit={:?}",
        result.as_ref().err(),
        result.as_ref().ok().map(|value| u8::from(value.exit_code)),
    );
    assert_eq!(filter.body_calls.load(Ordering::Relaxed), 3);
    assert!(!filter.fallback.load(Ordering::Relaxed));
    assert!(!filter.continued.load(Ordering::Relaxed));
    Ok(())
}

#[tokio::test]
async fn terminating_filter_stops_builtin_runaway_in_both_shell_modes() -> Result<()> {
    for interactive in [false, true] {
        assert_terminates("while true; do :; done; false", interactive).await?;
    }
    Ok(())
}

#[tokio::test]
async fn terminating_filter_escapes_nested_execution_recovery() -> Result<()> {
    for script in [
        "(while true; do :; done); false",
        "f() { while true; do :; done; }; f; false",
        "while true; do :; done | true; false",
        "eval 'while true; do :; done'; false",
    ] {
        assert_terminates(script, false).await?;
    }
    Ok(())
}

#[tokio::test]
async fn terminating_filter_survives_sourced_builtin_error_wrapping() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let script = temporary.path().join("runaway.sh");
    std::fs::write(&script, "while true; do :; done\n")?;
    let source = format!(
        ". {}; false",
        brush_core::escape::single_quote(&script.to_string_lossy())
    );
    assert_terminates(&source, false).await
}

#[tokio::test]
async fn ordinary_filter_errors_remain_recoverable() -> Result<()> {
    let filter = BudgetFilter::default();
    let mut shell = shell(filter.clone(), false).await?;
    let result = shell
        .run_string(
            "for item in a b c; do :; done; true",
            &brush_core::SourceInfo::default(),
            &shell.default_exec_params(),
        )
        .await?;
    assert!(result.is_success());
    assert_eq!(filter.body_calls.load(Ordering::Relaxed), 3);
    assert!(!filter.fallback.load(Ordering::Relaxed));
    Ok(())
}

#[tokio::test]
async fn terminating_debug_trap_prevents_command_and_cleans_temporary_environment() -> Result<()> {
    let filter = BudgetFilter {
        terminating: true,
        ..Default::default()
    };
    let mut shell = shell(filter.clone(), false).await?;
    let result = shell
        .run_string(
            "trap ':' DEBUG; FILTER_TEMP=one false",
            &brush_core::SourceInfo::default(),
            &shell.default_exec_params(),
        )
        .await;
    assert!(matches!(result, Err(ref error) if error.is_terminating()));
    assert!(!filter.continued.load(Ordering::Relaxed));
    assert_eq!(shell.env_str("FILTER_TEMP"), None);
    Ok(())
}

#[tokio::test]
async fn terminating_broken_pipe_is_not_reduced_to_a_builtin_exit_status() -> Result<()> {
    let filter = BudgetFilter {
        terminating: true,
        broken_pipe: true,
        ..Default::default()
    };
    let mut shell = shell(filter.clone(), false).await?;
    let result = shell
        .run_string(
            "eval ':'; false",
            &brush_core::SourceInfo::default(),
            &shell.default_exec_params(),
        )
        .await;
    assert!(matches!(result, Err(ref error) if error.is_terminating()));
    assert!(!filter.continued.load(Ordering::Relaxed));
    Ok(())
}

#[tokio::test]
async fn terminating_exit_trap_escapes_both_script_entrypoints() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("exit-trap.sh");
    let script = "trap ':' EXIT; true";
    std::fs::write(&path, script)?;
    for file in [false, true] {
        let filter = BudgetFilter {
            terminating: true,
            ..Default::default()
        };
        let mut shell = shell(filter.clone(), false).await?;
        let result = if file {
            shell.run_script(&path, std::iter::empty::<String>()).await
        } else {
            shell.run_dash_c_command(script).await
        };
        assert!(matches!(result, Err(ref error) if error.is_terminating()));
        assert_eq!(filter.body_calls.load(Ordering::Relaxed), 1);
    }
    Ok(())
}

#[cfg(feature = "builtin.printf")]
#[tokio::test]
async fn process_substitution_stops_denied_child_without_claiming_parent_failure() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    for deny in [false, true] {
        let marker = temporary
            .path()
            .join(if deny { "denied" } else { "allowed" });
        let filter = BudgetFilter {
            terminating: true,
            allowance: if deny { 0 } else { 100 },
            ..Default::default()
        };
        let mut shell = shell(filter.clone(), false).await?;
        let owners = Arc::strong_count(&filter.body_calls);
        let result = shell
            .run_string(
                format!(
                    "true <( :; printf effect > {} )",
                    brush_core::escape::single_quote(&marker.to_string_lossy()),
                ),
                &brush_core::SourceInfo::default(),
                &shell.default_exec_params(),
            )
            .await?;
        // Process substitution is an independent task today: its error is not
        // a synchronized parent result. Wait for the actual cloned policy
        // owner to be released before checking the child's filesystem effect.
        assert!(result.is_success());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while filter.body_calls.load(Ordering::Relaxed) == 0
            || Arc::strong_count(&filter.body_calls) > owners
        {
            assert!(
                std::time::Instant::now() < deadline,
                "substitution task did not finish"
            );
            tokio::task::yield_now().await;
        }
        assert_eq!(marker.exists(), !deny);
    }
    Ok(())
}

#[test]
fn termination_is_explicit_and_survives_builtin_wrapping() {
    let plain = Error::from(std::io::Error::other("ordinary failure"));
    assert!(!plain.is_terminating());
    assert!(!plain.into_fatal().is_terminating());
    let stopped = Error::from(std::io::Error::other("stop requested")).into_terminating();
    let wrapped = Error::from(brush_core::ErrorKind::BuiltinError(
        Box::new(stopped),
        "source".into(),
    ));
    assert!(wrapped.is_terminating());
}
