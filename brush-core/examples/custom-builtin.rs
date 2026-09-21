//! Example of implementing a custom builtin command for a brush-core based shell.
//!
//! `brush-core` depends on no argument-parsing engine, and neither does this
//! example: the builtin interprets its arguments and writes its help text by
//! hand. Those two things, plus `execute`, are the whole contract a builtin
//! has to satisfy, and this is close to the smallest builtin that satisfies it.
//!
//! For a builtin that uses clap to parse its arguments and render its help,
//! see the `clap-builtin` example in the `brush-builtin-utils` crate. Its
//! `clap_builtin!` macro implements steps 2 and 3 below from a `clap::Parser`
//! derive.
//!
//! Run this example with:
//! ```bash
//! cargo run --package brush-core --example custom-builtin
//! ```

use anyhow::Result;
use std::io::Write;

use brush_core::builtins::{self, ArgsError, FromArgs, HelpContent};
use brush_core::{CommandArg, ExecutionResult};

//
// Step 1: Define the builtin's state: what its arguments amount to.
// ==============================================
//

/// Greet the user, optionally more than once.
struct GreetCommand {
    repeat_count: usize,
}

//
// Step 2: Interpret the arguments.
// ==============================================
// `name` is what the builtin was invoked as; `args` are the words after it,
// exactly as the shell expanded them. Return `ArgsError::Usage` with a complete
// message for anything unacceptable; the shell prints it and fails the command
// with the usual usage status.
//

impl FromArgs for GreetCommand {
    fn from_args(name: &str, args: Vec<CommandArg>) -> Result<Self, ArgsError> {
        let words: Vec<String> = args.iter().map(ToString::to_string).collect();
        let words: Vec<&str> = words.iter().map(String::as_str).collect();

        match words.as_slice() {
            [] => Ok(Self { repeat_count: 1 }),
            ["-n", count] => count
                .parse()
                .map(|repeat_count| Self { repeat_count })
                .map_err(|_| {
                    ArgsError::Usage(format!("{name}: {count}: numeric argument required"))
                }),
            _ => Err(ArgsError::Usage(format!(
                "{name}: usage: {}",
                Self::synopsis(name)
            ))),
        }
    }
}

//
// Step 3: Provide help text.
// ==============================================
// `help -s` and `help -d` frame these as `name: synopsis` and `name - description`;
// the detailed help shown by plain `help name` defaults to a composition of the
// two, so a minimal builtin needs only these two lines.
//

impl HelpContent for GreetCommand {
    fn synopsis(name: &str) -> String {
        format!("{name} [-n count]")
    }

    fn description(_name: &str) -> String {
        "Greet the user".into()
    }
}

//
// Step 4: Implement the Command trait.
// ==============================================
// Only `execute` remains. It runs with the parsed state from step 2 and a
// context giving access to the shell and its I/O streams.
//

impl builtins::Command for GreetCommand {
    // The default error type suffices; see the `clap-builtin` example in
    // `brush-builtin-utils` for a custom one mapped to exit codes.
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let greeting = context
            .shell
            .basic_expand_string(&context.params, "Hello, $USER!")
            .await?;

        for _ in 0..self.repeat_count {
            writeln!(context.stdout(), "{greeting}")?;
        }

        Ok(ExecutionResult::success())
    }
}

//
// Step 5: Register the builtin with a shell.
// ==============================================
//

async fn run_example() -> Result<()> {
    let mut shell = brush_core::Shell::builder()
        .command::<GreetCommand>("greet")
        .build()
        .await?;

    for command in ["greet", "greet -n 2", "greet -n two"] {
        println!("$ {command}");
        let result = shell
            .run_string(
                command,
                &brush_core::SourceInfo::default(),
                &shell.default_exec_params(),
            )
            .await?;
        println!("(exit code: {})\n", u8::from(result.exit_code));
    }

    Ok(())
}

fn main() -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(run_example())
}
