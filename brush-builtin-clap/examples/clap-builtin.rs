//! Example of implementing a custom builtin command using clap, via
//! `brush-builtin-clap`.
//!
//! How this differs from `brush-core`'s `custom-builtin` example: that one
//! interprets its arguments and writes its help text by hand, which is the
//! whole contract a builtin has to satisfy and the smallest way to satisfy it.
//! This one hands both jobs to clap: the `impl_parse_args!` macro in step 3
//! implements the same two traits from a `clap::Parser` derive, so the builtin
//! gets option parsing, validation, and rendered help for free. It also shows
//! a builtin reporting its own failures in an error type of its own.
//! Start with the core example to learn the contract; use this one as the
//! template for a real builtin.
//!
//! This example demonstrates:
//! - Parsing command-line arguments with `clap` via `impl_parse_args!`
//! - Defining an error type for the builtin's own failures with `thiserror`
//! - Reporting one in a `builtins::Failure`, which keeps it
//! - Passing the shell's own errors through as they are
//! - Using the execution context to interact with shell state and I/O streams
//!
//! Run this example with:
//! ```bash
//! cargo run --package brush-builtin-clap --example clap-builtin
//! ```

use anyhow::Result;
use clap::Parser;
use std::io::Write;

use brush_core::{ExecutionResult, builtins};

//
// Step 1 (optional): Define an error type for your builtin's own failures
// ==============================================
// A builtin's `execute` returns a `brush_core::Error`, which the shell reports
// attributed to the builtin. A failure that is the builtin's own goes in a
// `builtins::Failure`, which keeps the error it's made from: an error
// formatter, or a test, gets it back with `Failure::downcast_ref`. We
// recommend `thiserror` for the error type.
//
// The shell's own errors aren't wrapped: they pass through `?` as they are,
// so a fatal one stays fatal and a broken pipe stays quiet.
//

/// What can go wrong in `greet`, in its own terms.
#[derive(Debug, thiserror::Error)]
enum GreetError {
    /// There's nobody to greet.
    #[error("nobody to greet: USER is empty")]
    NoUser,
}

//
// Step 2 (recommended): Define your builtin command arguments
// ==============================================
// We recommend using the `clap` crate and the derive-able `clap::Parser` to define
// command-line arguments and options. This will simplify the work you need to do
// to provide helpful usage information and auto-generated argument validation.
//

/// Greet the user with a friendly message.
#[derive(Parser)]
struct GreetCommand {
    /// Number of times to repeat the greeting (1 to 10).
    #[arg(
        short = 'n',
        long = "repeat",
        default_value_t = 1,
        value_parser = clap::value_parser!(u8).range(1..=10)
    )]
    repeat_count: u8,
}

//
// Step 3: Implement the StatelessBuiltin trait
// ==============================================
// `impl_parse_args!` wires up argument parsing and help from the clap derive;
// the `StatelessBuiltin` trait then requires implementing the `execute` method.
//

brush_builtin_clap::impl_parse_args!(GreetCommand);

impl<SE: brush_core::ShellExtensions> builtins::StatelessBuiltin<SE> for GreetCommand {
    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, brush_core::Error> {
        // A failure of the builtin's own, which the shell reports as
        // `greet: nobody to greet: USER is empty`. The command fails with
        // status 1; `Failure::with_exit_code` picks another.
        if context
            .shell
            .env_str("USER")
            .is_none_or(|user| user.is_empty())
        {
            return Err(builtins::Failure::new(GreetError::NoUser).into());
        }

        // For demonstration, we expand a greeting string using shell variable
        // expansion. This is a bit contrived, but an error from it (say,
        // `${USER?}` on an unset variable) is the shell's own, and `?` passes
        // it on as it is.
        let greeting = context
            .shell
            .basic_expand_string(&context.params, "Hello, ${USER}!")
            .await?;

        // Execute the greeting. A failed write is the shell's own error too.
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

async fn run_example() -> Result<()> {
    // Create a shell instance with the custom builtin registered.
    let mut shell = brush_core::Shell::builder()
        .builtin(
            "greet",
            brush_core::builtins::Registration::stateless::<GreetCommand>(),
        )
        .build()
        .await?;

    // Demonstrate basic usage, then the builtin's own failure.
    for script in ["USER=friend; greet -n 4", "USER=; greet"] {
        let result = shell
            .run_string(
                script,
                &brush_core::SourceInfo::default(),
                &shell.default_exec_params(),
            )
            .await?;
        println!("Exit code: {}\n", u8::from(result.exit_code));
    }

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
