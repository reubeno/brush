//! Tests of the state builtins keep in the shell between invocations.

#![cfg(test)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Result;
use brush_core::{ExecutionResult, builtins};

/// Adds the step its name gives (`by<step>`) to a running total, and returns
/// the total as its status.
struct Counter {}

/// The total the counters share: their state.
#[derive(Clone, Default)]
struct Total(u8);

/// A state no builtin uses.
#[derive(Clone, Default)]
struct UnusedState;

impl builtins::ParseArgs for Counter {
    fn content(
        name: &str,
        _content_type: builtins::ContentType,
        _options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::Error> {
        Ok(format!("{name} - add to a running total\n"))
    }

    fn parse(_invocation: builtins::Invocation<'_>) -> Result<Self, builtins::UsageError> {
        Ok(Self {})
    }
}

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for Counter {
    type Error = brush_core::Error;

    #[allow(
        clippy::unused_async_trait_impl,
        reason = "the trait's method is async; this one has nothing to wait for"
    )]
    async fn execute(
        self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let step: u8 = context
            .command_name
            .strip_prefix("by")
            .and_then(|step| step.parse().ok())
            .unwrap_or_default();
        let total = context.state_mut::<Total>();
        total.0 += step;
        Ok(ExecutionResult::new(total.0))
    }
}

async fn new_shell() -> Result<brush_core::Shell> {
    Ok(brush_core::Shell::builder()
        .builtin("by1", builtins::Registration::of::<Counter>())
        .builtin("by10", builtins::Registration::of::<Counter>())
        .build()
        .await?)
}

async fn run(shell: &mut brush_core::Shell, command: &str) -> Result<u8> {
    let params = shell.default_exec_params();
    let result = shell
        .run_string(command, &brush_core::SourceInfo::default(), &params)
        .await?;
    Ok(u8::from(result.exit_code))
}

#[tokio::test]
async fn a_builtin_s_state_follows_subshell_rules() -> Result<()> {
    let mut shell = new_shell().await?;
    // A subshell starts from the shell's state and keeps its own changes.
    for (command, expected) in [("by1", 1), ("(by1; by1)", 3), ("by1", 2)] {
        assert_eq!(run(&mut shell, command).await?, expected, "{command}");
    }
    // The state is readable from outside, by its type; one that no builtin
    // has used isn't there.
    assert_eq!(shell.builtin_state::<Total>().map(|t| t.0), Some(2));
    assert!(shell.builtin_state::<UnusedState>().is_none());
    Ok(())
}

#[tokio::test]
async fn a_state_is_kept_by_its_type_whatever_the_builtin_s_name() -> Result<()> {
    let mut shell = new_shell().await?;
    assert_eq!(run(&mut shell, "by1").await?, 1);
    assert_eq!(run(&mut shell, "by10").await?, 11);
    assert_eq!(run(&mut shell, "by1").await?, 12);
    Ok(())
}

#[tokio::test]
async fn a_state_set_from_outside_is_where_builtins_start() -> Result<()> {
    let mut shell = new_shell().await?;
    // A builtin starts from the state set, not from its default.
    shell.set_builtin_state(Total(40));
    assert_eq!(run(&mut shell, "by1").await?, 41);
    // Setting it again replaces the state a builtin changed.
    shell.set_builtin_state(Total(5));
    assert_eq!(run(&mut shell, "by10").await?, 15);
    assert_eq!(shell.builtin_state::<Total>().map(|t| t.0), Some(15));
    Ok(())
}
