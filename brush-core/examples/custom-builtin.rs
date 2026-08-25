//! Example of implementing a custom builtin command for a brush-core based shell.
//!
//! This example demonstrates best practices for:
//! - Creating a custom builtin command using the `SpecCommand` trait
//! - Declaring arguments as backend-neutral data (`argmodel`)
//! - Defining custom error types with `thiserror`
//! - Implementing proper error handling and exit code conversion
//! - Using the execution context to interact with shell state and I/O streams
//!
//! Run this example with:
//! ```bash
//! cargo run --package brush-core --example custom-builtin
//! ```

use anyhow::Result;
use std::io::Write;

use brush_core::{ExecutionResult, builtins};

//
// Step 1 (optional): Define a custom error type for your builtin
// ==============================================
// We recommend using `thiserror` to create descriptive error types that can be converted
// to appropriate exit codes.
//

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

// Mark your error type as a builtin error. This is required to use this error
// type in your command implementation.
impl brush_core::BuiltinError for GreetError {}

// If you define a custom error type, you must map each error variant to an appropriate
// exit code. This ensures the shell interpreter will translate a returned error to
// the appropriate code during execution.
impl From<&GreetError> for brush_core::ExecutionExitCode {
    fn from(value: &GreetError) -> Self {
        match value {
            GreetError::RepeatCountOutOfRange => Self::InvalidUsage,
            GreetError::ShellError(e) => e.into(),
            GreetError::IoError(_) => Self::GeneralError,
        }
    }
}

//
// Step 2 (recommended): Declare your builtin command arguments as data.
// ==============================================
// The `SpecCommand` trait takes a backend-neutral description (`CommandSpec`)
// and hands back parsed values (`Matches`). Which crate parses them (bpaf,
// usage, clap) is selected by cargo features of `brush-core`.

const ID_REPEAT: &str = "repeat_count";

struct GreetCommand {
    repeat_count: usize,
}

//
// Step 3: Implement the SpecCommand trait.
//

impl builtins::SpecCommand for GreetCommand {
    type Error = GreetError;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec.arg(
            ID_REPEAT,
            &['n'],
            &["repeat"],
            builtins::argmodel::ArgKind::Value,
            Some("COUNT"),
            "Number of times to repeat the greeting.",
        )
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        let value = matches.value(ID_REPEAT).unwrap_or("1");
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
    ) -> Result<ExecutionResult, Self::Error> {
        // Additional validation.
        if self.repeat_count == 0 || self.repeat_count > 10 {
            return Err(GreetError::RepeatCountOutOfRange);
        }

        // For demonstration, we expand a greeting string using shell variable expansion.
        // This is a bit contrived, but it shows how to wrap errors coming back from
        // `brush_core`.
        let greeting = context
            .shell
            .basic_expand_string(&context.params, "Hello, ${USER}!")
            .await?;

        // Execute the greeting.
        for _ in 0..self.repeat_count {
            writeln!(context.stdout(), "{greeting}")?;
        }

        // Return success
        Ok(ExecutionResult::success())
    }
}

//
// Step 4: Integrate your builtin into a shell
// ==============================================
// This example shows how to register and use your custom builtin.
//

type SE = brush_core::extensions::DefaultShellExtensions;

async fn run_example() -> Result<()> {
    // Create a shell instance with custom builtin registered.
    let mut shell = brush_core::Shell::builder()
        .builtin(
            "greet",
            brush_core::builtins::spec_builtin::<GreetCommand, SE>(),
        )
        .build()
        .await?;

    // Demonstrate basic usage.
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
    // Construct a `tokio` runtime for async execution
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(run_example())?;

    Ok(())
}
