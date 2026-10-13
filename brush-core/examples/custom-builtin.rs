//! Example of implementing custom builtin commands for a brush-core based shell.
//!
//! `brush-core` depends on no argument-parsing engine, and neither does this
//! example: its builtins interpret their arguments and describe their help by
//! hand. That, plus running, is the whole contract a builtin has to satisfy.
//!
//! It shows the contract's two shapes:
//!
//! - `greet` is a [`builtins::StatelessBuiltin`]: it has no state of its own,
//!   and its arguments type is the whole builtin. Most builtins look like this.
//!   It also reports failures of its own, in an error type of its own with an
//!   exit code for each variant.
//! - `tally` is a [`builtins::Builtin`]: the registered value is the builtin, so
//!   it can hold state or configuration of its own; here, a running count.
//!
//! For builtins whose arguments are parsed by a library, see the
//! `clap-builtin` example in `brush-builtin-clap`, whose `impl_parse_args!`
//! implements [`ParseArgs`] from a derive.
//!
//! Run this example with:
//! ```bash
//! cargo run --package brush-core --example custom-builtin
//! ```

use anyhow::Result;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};

use brush_core::builtins::{self, Help, Invocation, ParseArgs, Registration, UsageError};
use brush_core::{ExecutionExitCode, ExecutionResult};

//
// A stateless builtin: its arguments type is the whole builtin.
// ==============================================
//

/// What can go wrong in `greet`, in its own terms.
#[derive(Debug, thiserror::Error)]
enum GreetError {
    /// There's nobody to greet.
    #[error("nobody to greet: USER is empty")]
    NoUser,

    /// More greetings were asked for than `GREET_LIMIT` allows.
    #[error("{requested}: more greetings than GREET_LIMIT ({limit}) allows")]
    OverLimit { requested: usize, limit: usize },
}

// A failure of the builtin's own goes in a `builtins::Failure`, which keeps the
// error it's made from: an error formatter, or a test, gets it back with
// `Failure::downcast_ref`. Converting the type into a `brush_core::Error` in
// one place, choosing each variant's exit code there, lets `?` work in
// `execute`. (The shell's own errors aren't wrapped: they pass through `?` as
// they are, so a fatal one stays fatal and a broken pipe stays quiet.)
impl From<GreetError> for brush_core::Error {
    fn from(error: GreetError) -> Self {
        let exit_code = match &error {
            GreetError::NoUser => ExecutionExitCode::GeneralError,
            GreetError::OverLimit { .. } => ExecutionExitCode::InvalidUsage,
        };
        builtins::Failure::new(error)
            .with_exit_code(exit_code)
            .into()
    }
}

/// Greet the user, optionally more than once.
struct GreetCommand {
    repeat_count: usize,
}

// Describe the builtin, and interpret its arguments. `invocation` holds the name
// the builtin was invoked as and the words after it, exactly as the shell
// expanded them. Return a `UsageError` for anything unacceptable: the shell
// reports it as `greet: <message>`, adds the usage line built from the help, and
// fails the command with the usual usage status.
impl ParseArgs for GreetCommand {
    fn help(name: &str) -> Help {
        // `help -s`, `help -d`, `help greet`, and `greet --help` all render this.
        Help::new(format!("{name} [-n count]"), "Greet the user.").with_details(
            "Prints a greeting COUNT times (once, by default), up to GREET_LIMIT (10).",
        )
    }

    fn parse(invocation: Invocation<'_>) -> Result<Self, UsageError> {
        let words = invocation.into_words();
        let words: Vec<&str> = words.iter().map(String::as_str).collect();

        match words.as_slice() {
            [] => Ok(Self { repeat_count: 1 }),
            ["--help"] => Err(UsageError::help_requested()),
            ["-n"] => Err(UsageError::requires_argument("-n")),
            ["-n", count] => count
                .parse()
                .map(|repeat_count| Self { repeat_count })
                .map_err(|_| UsageError::new(format!("{count}: numeric argument required"))),
            [option, ..] if option.starts_with('-') => Err(UsageError::invalid_option(option)),
            _ => Err(UsageError::new("too many arguments").with_usage_line()),
        }
    }
}

// Run it, with the parsed arguments and a context giving access to the shell
// and its I/O streams.
impl<SE: brush_core::ShellExtensions> builtins::StatelessBuiltin<SE> for GreetCommand {
    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, brush_core::Error> {
        // Failures of the builtin's own, which depend on the shell's state, so
        // they're found here rather than in `parse`. The shell reports them as
        // `greet: <message>`, and the command fails with the variant's code.
        let user = context.shell.env_str("USER").unwrap_or_default();
        if user.is_empty() {
            return Err(GreetError::NoUser.into());
        }
        let limit = context
            .shell
            .env_str("GREET_LIMIT")
            .and_then(|limit| limit.parse().ok())
            .unwrap_or(10);
        if self.repeat_count > limit {
            return Err(GreetError::OverLimit {
                requested: self.repeat_count,
                limit,
            }
            .into());
        }

        // Errors from the shell's own work, expanding a word or writing, are the
        // shell's; `?` passes them on as they are.
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
// A builtin with state of its own.
// ==============================================
//

/// Counts its calls, under a configured label.
struct Tally {
    label: &'static str,
    calls: AtomicUsize,
}

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for Tally {
    // What one invocation's arguments parse to: here, nothing.
    type Args = ();
    fn help(&self, name: &str) -> Help {
        Help::new(name, format!("Count {} calls.", self.label))
    }

    fn parse(&self, invocation: Invocation<'_>) -> Result<(), UsageError> {
        match invocation.into_words().as_slice() {
            [] => Ok(()),
            [help] if help == "--help" => Err(UsageError::help_requested()),
            _ => Err(UsageError::new("takes no arguments").with_usage_line()),
        }
    }

    #[allow(
        clippy::unused_async_trait_impl,
        reason = "the trait's method is async; this one has nothing to wait for"
    )]
    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
        (): (),
    ) -> Result<ExecutionResult, brush_core::Error> {
        let calls = self.calls.fetch_add(1, Ordering::Relaxed) + 1;
        writeln!(context.stdout(), "{} calls: {calls}", self.label)?;
        Ok(ExecutionResult::success())
    }
}

//
// Register the builtins with a shell.
// ==============================================
//

async fn run_example() -> Result<()> {
    let tally = |label| Tally {
        label,
        calls: AtomicUsize::new(0),
    };

    let mut shell = brush_core::Shell::builder()
        .builtin("greet", Registration::stateless::<GreetCommand>())
        // One builtin type, registered twice with different state.
        .builtin("tally-a", Registration::new(tally("a")))
        .builtin("tally-b", Registration::new(tally("b")))
        .build()
        .await?;

    for command in [
        "USER=friend; greet",
        "greet -n 2",
        "greet -n two",
        "greet -x",
        "greet --help",
        "greet -n 11",
        "USER=; greet",
        "tally-a; tally-a; tally-b",
        "tally-a --help",
    ] {
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
