//! Example of implementing a custom builtin with usage-rs.
//!
//! The contract is the same one `brush-core`'s `custom-builtin` example
//! satisfies by hand and `brush-builtin-utils`'s `clap-builtin` example
//! satisfies with clap. This one derives [`usage::Cli`] and lets
//! [`brush_builtin_usage::usage_builtin`] implement argument parsing and help.
//!
//! Run this example with:
//!
//! ```bash
//! cargo run --package brush-builtin-usage --example usage-builtin
//! ```

use anyhow::Result;
use std::io::Write;
use usage::Cli;

use brush_core::{ExecutionResult, builtins};

#[derive(Debug, thiserror::Error)]
enum GreetError {
    /// The requested repeat count is beyond the supported range.
    #[error("repeat count out of range")]
    RepeatCountOutOfRange,

    /// A shell error occurred during execution.
    #[error(transparent)]
    ShellError(#[from] brush_core::Error),

    /// An I/O error occurred.
    #[error("I/O error occurred during greeting: {0}")]
    IoError(#[from] std::io::Error),
}

impl brush_core::BuiltinError for GreetError {}

impl From<&GreetError> for brush_core::ExecutionExitCode {
    fn from(value: &GreetError) -> Self {
        match value {
            GreetError::RepeatCountOutOfRange => Self::InvalidUsage,
            GreetError::ShellError(error) => error.into(),
            GreetError::IoError(_) => Self::GeneralError,
        }
    }
}

/// Greet the user with a friendly message.
#[derive(Cli)]
#[usage(bin = "greet")]
struct GreetCommand {
    /// Number of times to repeat the greeting.
    #[usage(short = 'n', long = "repeat")]
    repeat_count: Option<usize>,
}

brush_builtin_usage::usage_builtin!(GreetCommand);

impl builtins::Command for GreetCommand {
    type Error = GreetError;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let repeat_count = self.repeat_count.unwrap_or(1);
        if repeat_count == 0 || repeat_count > 10 {
            return Err(GreetError::RepeatCountOutOfRange);
        }

        let greeting = context
            .shell
            .basic_expand_string(&context.params, "Hello, ${USER}!")
            .await?;

        for _ in 0..repeat_count {
            writeln!(context.stdout(), "{greeting}")?;
        }

        Ok(ExecutionResult::success())
    }
}

async fn run_example() -> Result<()> {
    let mut shell = brush_core::Shell::builder()
        .command::<GreetCommand>("greet")
        .build()
        .await?;

    let result = shell
        .run_string(
            "greet -n 4",
            &brush_core::SourceInfo::default(),
            &shell.default_exec_params(),
        )
        .await?;
    println!("Exit code: {}\n", u8::from(result.exit_code));

    Ok(())
}

fn main() -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(run_example())?;

    Ok(())
}
