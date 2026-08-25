//! Example of implementing a custom builtin command for a brush-core based shell.
//!
//! This example demonstrates best practices for:
//! - Creating a custom builtin command using the `SpecCommand` trait
//! - Declaring arguments as compile-time data (`argmodel`)
//! - Defining custom error types with `thiserror`
//!
//! Run this example with:
//! ```bash
//! cargo run --package brush-core --example custom-builtin
//! ```

use anyhow::Result;
use brush_core::builtins;
use std::io::Write;

//
// Step 1 (optional): Define a custom error type for your builtin
// ==============================================

#[derive(Debug, thiserror::Error)]
enum GreetError {
    /// The requested repeat count is beyond the supported range.
    #[error("repeat count out of range")]
    RepeatCountOutOfRange,

    /// A shell error occurred during execution; we transparently forward error display
    /// to the underlying error.
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
            GreetError::ShellError(e) => e.into(),
            GreetError::IoError(_) => Self::GeneralError,
        }
    }
}

// Step 2: Declare your builtin's arguments as compile-time data.
// ==============================================
// `SpecCommand::spec()` returns a static `CommandSpec`; whichever argument-
// parsing crate brush-core was built with (bpaf, usage, clap) turns it into
// an actual parser.

const ID_REPEAT: &str = "repeat_count";

struct GreetCommand {
    repeat_count: usize,
}

static SPEC: builtins::argmodel::CommandSpec = builtins::argmodel::CommandSpec {
    args: &[builtins::argmodel::ArgSpec::value(
        ID_REPEAT,
        &['n'],
        &["repeat"],
        "COUNT",
        "Number of times to repeat the greeting.",
    )],
    positionals: &[],
};

//
// Step 3: Implement the SpecCommand trait.
//

impl builtins::SpecCommand for GreetCommand {
    type Error = GreetError;

    fn spec() -> &'static builtins::argmodel::CommandSpec {
        &SPEC
    }

    fn from_matches(
        values: &mut builtins::argmodel::ParsedValues,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        let value = values.value(ID_REPEAT).unwrap_or("1");
        let repeat_count: usize = value.parse().map_err(|_| builtins::BuiltinArgParseError {
            message: format!("invalid repeat count: `{value}`"),
            help_request: false,
        })?;
        Ok(Self { repeat_count })
    }

    fn about() -> &'static str {
        "Greet the user with a friendly message."
    }

    fn synopsis() -> &'static str {
        "[-n COUNT]"
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        // Additional validation.
        if self.repeat_count == 0 || self.repeat_count > 10 {
            return Err(GreetError::RepeatCountOutOfRange);
        }

        let greeting = context
            .shell
            .basic_expand_string(&context.params, "Hello, ${USER}!")
            .await?;

        for _ in 0..self.repeat_count {
            writeln!(context.stdout(), "{greeting}")?;
        }

        Ok(brush_core::ExecutionResult::success())
    }
}

type SE = brush_core::extensions::DefaultShellExtensions;

async fn run_example() -> Result<()> {
    let mut shell = brush_core::Shell::builder()
        .builtin(
            "greet",
            brush_core::builtins::spec_builtin::<GreetCommand, SE>(),
        )
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
