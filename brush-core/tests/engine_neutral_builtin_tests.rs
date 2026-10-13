//! Proves the builtin contract is engine-neutral: these builtins are parsed and
//! documented entirely by hand, with no argument-parsing engine anywhere in
//! them, and the shell runs them like any other.
//!
//! `brush-core` has no dependency on any parsing engine, so this is the only way
//! a builtin can be written from within this crate's tests. If the contract ever
//! grows an engine-specific requirement, this file stops compiling.

#![cfg(test)]
#![allow(
    clippy::panic_in_result_fn,
    clippy::expect_used,
    clippy::unused_async_trait_impl
)]

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use brush_core::builtins::{self, Declarations, Help, HelpForm, Invocation, ParseArgs, UsageError};
use brush_core::{CommandArg, ExecutionResult};

type SE = brush_core::extensions::DefaultShellExtensions;

/// Reports how many operands it was given, optionally scaled by `-n`, as its
/// exit code. Contrived, but it exercises an option with a value, an operand
/// list, and a usage error without needing to capture output.
struct CountCommand {
    scale: u8,
    operands: usize,
}

impl ParseArgs for CountCommand {
    fn help(name: &str) -> Help {
        Help::new(format!("{name} [-n scale] [operand...]"), "count operands")
    }

    fn parse(invocation: Invocation<'_>) -> Result<Self, UsageError> {
        let mut scale = 1;
        let mut operands = 0;
        let mut words = invocation.into_words().into_iter();

        while let Some(word) = words.next() {
            match word.as_str() {
                "--help" => return Err(UsageError::help_requested()),
                "-n" => {
                    let value = words
                        .next()
                        .ok_or_else(|| UsageError::requires_argument("-n"))?;
                    scale = value.parse().map_err(|_| {
                        UsageError::new(format!("{value}: numeric argument required"))
                    })?;
                }
                _ if word.starts_with('-') && word.len() > 1 => {
                    return Err(UsageError::invalid_option(word));
                }
                _ => operands += 1,
            }
        }

        Ok(Self { scale, operands })
    }
}

impl<S: brush_core::ShellExtensions> builtins::StatelessBuiltin<S> for CountCommand {
    async fn execute(
        self,
        _context: brush_core::ExecutionContext<'_, S>,
    ) -> Result<ExecutionResult, brush_core::Error> {
        #[expect(clippy::cast_possible_truncation)]
        Ok(ExecutionResult::new(self.operands as u8 * self.scale))
    }
}

/// A declaration utility that reports what it received as its exit code: tens
/// for declarations, ones for strings. It does no option handling of its own, so
/// any splitting or dropping of dash-prefixed words would have to be the shell's.
struct DeclCountCommand {
    declarations: u8,
    strings: u8,
}

impl ParseArgs for DeclCountCommand {
    fn help(name: &str) -> Help {
        Help::new(format!("{name} [arg...]"), "count declarations")
    }

    fn parse(invocation: Invocation<'_>) -> Result<Self, UsageError> {
        let mut this = Self {
            declarations: 0,
            strings: 0,
        };

        for arg in invocation.args() {
            match arg {
                CommandArg::String(_) => this.strings += 1,
                CommandArg::Declaration(_) => this.declarations += 1,
            }
        }

        Ok(this)
    }
}

impl<S: brush_core::ShellExtensions> builtins::StatelessBuiltin<S> for DeclCountCommand {
    const DECLARATIONS: Declarations = Declarations::Yes;

    async fn execute(
        self,
        _context: brush_core::ExecutionContext<'_, S>,
    ) -> Result<ExecutionResult, brush_core::Error> {
        Ok(ExecutionResult::new(self.declarations * 10 + self.strings))
    }
}

/// A builtin with state of its own: it counts its calls, adding a configured
/// step each time, and reports the running total as its exit code. The state is
/// in the registered value, so two registrations of it are independent.
struct Counter {
    step: u8,
    total: AtomicU8,
}

impl<S: brush_core::ShellExtensions> builtins::Builtin<S> for Counter {
    type Args = ();

    fn help(&self, name: &str) -> Help {
        Help::new(name, format!("count by {}", self.step))
    }

    fn parse(&self, _invocation: Invocation<'_>) -> Result<(), UsageError> {
        Ok(())
    }

    async fn execute(
        &self,
        _context: brush_core::ExecutionContext<'_, S>,
        (): (),
    ) -> Result<ExecutionResult, brush_core::Error> {
        let total = self.total.fetch_add(self.step, Ordering::Relaxed) + self.step;
        Ok(ExecutionResult::new(total))
    }
}

/// Runs the builtin its operands name twice, through the shell, and reports the
/// second run's status, so a builtin can run more than one builtin.
struct Twice {
    argv: Vec<CommandArg>,
}

impl ParseArgs for Twice {
    fn help(name: &str) -> Help {
        Help::new(format!("{name} builtin [arg...]"), "run a builtin twice")
    }

    fn parse(invocation: Invocation<'_>) -> Result<Self, UsageError> {
        Ok(Self {
            argv: invocation.into_args(),
        })
    }
}

impl<S: brush_core::ShellExtensions> builtins::StatelessBuiltin<S> for Twice {
    async fn execute(
        self,
        mut context: brush_core::ExecutionContext<'_, S>,
    ) -> Result<ExecutionResult, brush_core::Error> {
        // Control flow from the first run (an `exit`, say) is passed on, not
        // dropped.
        let first = context.run_builtin(self.argv.iter().cloned()).await?;
        if !first.is_normal_flow() {
            return Ok(first);
        }
        context.run_builtin(self.argv).await
    }
}

/// Fails with a status of 3, in its own words.
struct Fails;

impl ParseArgs for Fails {
    fn help(name: &str) -> Help {
        Help::new(name, "fail")
    }

    fn parse(_invocation: Invocation<'_>) -> Result<Self, UsageError> {
        Ok(Self)
    }
}

impl<S: brush_core::ShellExtensions> builtins::StatelessBuiltin<S> for Fails {
    async fn execute(
        self,
        _context: brush_core::ExecutionContext<'_, S>,
    ) -> Result<ExecutionResult, brush_core::Error> {
        Err(builtins::Failure::new("it failed").with_exit_code(3).into())
    }
}

/// An error type of a builtin's own, with a cause.
#[derive(Debug)]
struct NoTable {
    name: String,
    cause: std::io::Error,
}

impl std::fmt::Display for NoTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: no such table", self.name)
    }
}

impl std::error::Error for NoTable {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

fn no_table(name: &str) -> NoTable {
    NoTable {
        name: name.into(),
        cause: std::io::ErrorKind::NotFound.into(),
    }
}

/// Fails with an error of its own type.
struct Lookup;

impl ParseArgs for Lookup {
    fn help(name: &str) -> Help {
        Help::new(name, "look up a table")
    }

    fn parse(_invocation: Invocation<'_>) -> Result<Self, UsageError> {
        Ok(Self)
    }
}

impl<S: brush_core::ShellExtensions> builtins::StatelessBuiltin<S> for Lookup {
    async fn execute(
        self,
        _context: brush_core::ExecutionContext<'_, S>,
    ) -> Result<ExecutionResult, brush_core::Error> {
        Err(builtins::Failure::new(no_table("t"))
            .with_exit_code(4)
            .into())
    }
}

/// Fails with an error of its own type, caused by a broken pipe.
struct PipeGone;

impl ParseArgs for PipeGone {
    fn help(name: &str) -> Help {
        Help::new(name, "write to a table that's gone")
    }

    fn parse(_invocation: Invocation<'_>) -> Result<Self, UsageError> {
        Ok(Self)
    }
}

impl<S: brush_core::ShellExtensions> builtins::StatelessBuiltin<S> for PipeGone {
    async fn execute(
        self,
        _context: brush_core::ExecutionContext<'_, S>,
    ) -> Result<ExecutionResult, brush_core::Error> {
        let error = NoTable {
            name: "t".into(),
            cause: std::io::ErrorKind::BrokenPipe.into(),
        };
        Err(builtins::Failure::new(error).with_exit_code(4).into())
    }
}

/// An error formatter that knows `Lookup`'s error type, as an embedder's might,
/// and records what it finds in the errors it formats.
#[derive(Clone, Default)]
struct TableErrors(Arc<Mutex<Vec<String>>>);

impl brush_core::extensions::ErrorFormatter for TableErrors {
    fn format_error(
        &self,
        error: &brush_core::Error,
        _shell: &brush_core::Shell<impl brush_core::ShellExtensions>,
    ) -> String {
        if let brush_core::ErrorKind::BuiltinError { builtin, error, .. } = error.kind()
            && let brush_core::ErrorKind::BuiltinFailure(failure) = error.kind()
            && let Some(no_table) = failure.downcast_ref::<NoTable>()
        {
            let found = format!("{builtin}: table {} ({})", no_table.name, no_table.cause);
            self.0.lock().expect("not poisoned").push(found);
        }
        format!("error: {error}\n")
    }
}

/// Implemented for one shell's extensions type alone, so it can call code that
/// takes that shell's concrete type, as an embedder's builtin might.
struct ForOneShell;

/// Code of an embedder's that takes its own shell's concrete type.
fn positional_count(shell: &brush_core::Shell<SE>) -> u8 {
    u8::try_from(shell.current_shell_args().len()).unwrap_or(u8::MAX)
}

impl ParseArgs for ForOneShell {
    fn help(name: &str) -> Help {
        Help::new(name, "run in one kind of shell")
    }

    fn parse(_invocation: Invocation<'_>) -> Result<Self, UsageError> {
        Ok(Self)
    }
}

impl builtins::StatelessBuiltin<SE> for ForOneShell {
    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, brush_core::Error> {
        Ok(ExecutionResult::new(positional_count(context.shell)))
    }
}

/// Renders its help for a terminal, as an engine adapter does.
struct Painted;

impl ParseArgs for Painted {
    fn help(name: &str) -> Help {
        Help::new(name, "painted for a terminal")
    }

    fn terminal_help(name: &str, style: &builtins::TerminalStyle) -> Option<Help> {
        let width = style.width;
        Some(Help::new(
            format!("\x1b[1m{name}\x1b[0m"),
            format!("{width} columns"),
        ))
    }

    fn parse(_invocation: Invocation<'_>) -> Result<Self, UsageError> {
        Ok(Self)
    }
}

impl<S: brush_core::ShellExtensions> builtins::StatelessBuiltin<S> for Painted {
    async fn execute(
        self,
        _context: brush_core::ExecutionContext<'_, S>,
    ) -> Result<ExecutionResult, brush_core::Error> {
        Ok(ExecutionResult::success())
    }
}

async fn shell() -> Result<brush_core::Shell<SE>> {
    let counter = |step| Counter {
        step,
        total: AtomicU8::new(0),
    };

    Ok(brush_core::Shell::builder()
        .builtin("count", builtins::Registration::stateless::<CountCommand>())
        // Builtins are special by name; these test builtins borrow special
        // builtins' names.
        .builtin("times", builtins::Registration::stateless::<CountCommand>())
        .builtin("trap", builtins::Registration::stateless::<Fails>())
        .builtin("fails", builtins::Registration::stateless::<Fails>())
        .builtin(
            "declcount",
            builtins::Registration::stateless::<DeclCountCommand>(),
        )
        .builtin("twice", builtins::Registration::stateless::<Twice>())
        .builtin(
            "for-one-shell",
            builtins::Registration::stateless::<ForOneShell>(),
        )
        .builtin("by1", builtins::Registration::new(counter(1)))
        .builtin("by10", builtins::Registration::new(counter(10)))
        .build()
        .await?)
}

async fn run(shell: &mut brush_core::Shell<SE>, command: &str) -> Result<ExecutionResult> {
    let params = shell.default_exec_params();
    Ok(shell
        .run_string(command, &brush_core::SourceInfo::default(), &params)
        .await?)
}

async fn exit_code_of(command: &str) -> Result<u8> {
    Ok(u8::from(run(&mut shell().await?, command).await?.exit_code))
}

#[tokio::test]
async fn hand_parsed_builtin_receives_operands_without_its_own_name() -> Result<()> {
    // Three operands, and the builtin's own name is not one of them.
    assert_eq!(exit_code_of("count a b c").await?, 3);
    Ok(())
}

#[tokio::test]
async fn hand_parsed_builtin_reads_an_option_value() -> Result<()> {
    assert_eq!(exit_code_of("count -n 2 a b").await?, 4);
    Ok(())
}

#[tokio::test]
async fn usage_errors_fail_with_the_usage_status() -> Result<()> {
    assert_eq!(exit_code_of("count -z").await?, 2);
    assert_eq!(exit_code_of("count -n").await?, 2);
    Ok(())
}

#[tokio::test]
async fn help_request_exits_with_usage_status() -> Result<()> {
    // As in bash, `--help` prints the help (on stdout) and fails with the usage
    // status.
    assert_eq!(exit_code_of("count --help").await?, 2);
    Ok(())
}

#[tokio::test]
async fn failures_fail_with_their_exit_code() -> Result<()> {
    assert_eq!(exit_code_of("fails").await?, 3);
    Ok(())
}

#[tokio::test]
async fn an_error_formatter_gets_a_failure_s_own_error_back() -> Result<()> {
    type WithTableErrors = brush_core::extensions::ShellExtensionsImpl<TableErrors>;
    let formatter = TableErrors::default();
    let mut shell = brush_core::Shell::builder_with_extensions::<WithTableErrors>()
        .builtin("lookup", builtins::Registration::stateless::<Lookup>())
        .error_formatter(formatter.clone())
        .build()
        .await?;

    let params = shell.default_exec_params();
    let result = shell
        .run_string("lookup", &brush_core::SourceInfo::default(), &params)
        .await?;

    assert_eq!(u8::from(result.exit_code), 4);
    assert_eq!(
        *formatter.0.lock().expect("not poisoned"),
        ["lookup: table t (entity not found)"]
    );
    Ok(())
}

#[tokio::test]
async fn a_failure_caused_by_a_broken_pipe_is_not_printed() -> Result<()> {
    type WithTableErrors = brush_core::extensions::ShellExtensionsImpl<TableErrors>;
    let formatter = TableErrors::default();
    let mut shell = brush_core::Shell::builder_with_extensions::<WithTableErrors>()
        .builtin("pipe-gone", builtins::Registration::stateless::<PipeGone>())
        .error_formatter(formatter.clone())
        .build()
        .await?;

    let params = shell.default_exec_params();
    let result = shell
        .run_string("pipe-gone", &brush_core::SourceInfo::default(), &params)
        .await?;

    // As for a broken pipe the shell sees: the broken pipe's status, unprinted.
    let broken_pipe = std::io::Error::from(std::io::ErrorKind::BrokenPipe);
    assert_eq!(
        u8::from(result.exit_code),
        u8::from(brush_core::ExecutionExitCode::from(&broken_pipe))
    );
    assert!(formatter.0.lock().expect("not poisoned").is_empty());
    Ok(())
}

#[tokio::test]
async fn only_errors_a_special_builtin_returns_are_fatal_in_posix_mode() -> Result<()> {
    let mut shell = shell().await?;
    shell.options_mut().posix_mode = true;

    // An error `trap` returns ends a non-interactive shell; `fails`, the same
    // builtin under a name that isn't special, doesn't.
    assert!(run(&mut shell, "trap").await?.is_exit());
    let mut shell = self::shell().await?;
    shell.options_mut().posix_mode = true;
    assert!(!run(&mut shell, "fails").await?.is_exit());

    // A usage error is reported, and doesn't end the shell.
    let mut shell = self::shell().await?;
    shell.options_mut().posix_mode = true;
    assert!(!run(&mut shell, "times -z").await?.is_exit());

    // Run through `builtin`, a special builtin loses its special properties.
    let mut shell = self::shell().await?;
    shell.options_mut().posix_mode = true;
    assert!(!run(&mut shell, "twice trap").await?.is_exit());
    Ok(())
}

#[tokio::test]
async fn builtins_run_other_builtins_through_the_shell() -> Result<()> {
    // Each run counts, so a builtin can run more than one.
    let mut shell = shell().await?;
    assert_eq!(u8::from(run(&mut shell, "twice by1").await?.exit_code), 2);

    // The inner builtin's usage errors show up as its status, as they would if
    // it were run directly.
    assert_eq!(exit_code_of("twice count -z").await?, 2);

    // Not finding a builtin is the caller's own failure.
    assert_eq!(exit_code_of("twice no-such-builtin").await?, 1);
    Ok(())
}

#[tokio::test]
async fn a_builtin_can_be_for_one_kind_of_shell() -> Result<()> {
    // `ForOneShell` implements `StatelessBuiltin<SE>` for this file's `SE`
    // alone, and calls code that takes a `Shell<SE>`.
    assert_eq!(exit_code_of("f() { for-one-shell; }; f a b").await?, 2);
    Ok(())
}

#[tokio::test]
async fn shell_passes_arguments_through_without_option_handling() -> Result<()> {
    // Two declarations and three strings, including dash-prefixed words both
    // before and after the declarations: nothing is split off, dropped, or
    // flattened on the builtin's behalf.
    assert_eq!(exit_code_of("declcount -x a=1 -r b=2 c").await?, 23);
    Ok(())
}

#[tokio::test]
async fn assignments_stay_strings_unless_the_builtin_is_a_declaration_utility() -> Result<()> {
    // `count` isn't a declaration utility, so `a=1` reaches it as a plain operand.
    assert_eq!(exit_code_of("count a=1 b").await?, 2);
    Ok(())
}

#[tokio::test]
async fn builtins_hold_their_own_state() -> Result<()> {
    let mut shell = shell().await?;
    for (command, expected) in [("by1", 1), ("by10", 10), ("by1", 2), ("by10", 20)] {
        assert_eq!(
            u8::from(run(&mut shell, command).await?.exit_code),
            expected
        );
    }
    Ok(())
}

#[test]
fn help_is_rendered_for_a_terminal_only_when_there_is_one() {
    use builtins::TerminalStyle;

    let painted = builtins::Registration::<SE>::stateless::<Painted>();
    let style = TerminalStyle::new(96, true);
    let help = painted.help("p", Some(&style));
    assert_eq!(help.synopsis, "\x1b[1mp\x1b[0m");
    assert_eq!(help.description, "96 columns");
    assert_eq!(
        painted.help("p", None).description,
        "painted for a terminal"
    );

    // A builtin that renders nothing for a terminal is printed plain.
    let plain = builtins::Registration::<SE>::stateless::<CountCommand>();
    assert_eq!(plain.help("count", Some(&style)), plain.help("count", None));
}

#[test]
fn registration_help_renders_in_every_form() {
    let registration = builtins::Registration::<SE>::stateless::<CountCommand>();
    let help = registration.help("count", None);

    assert_eq!(
        help.render("count", HelpForm::Synopsis),
        "count: count [-n scale] [operand...]\n"
    );
    assert_eq!(
        help.render("count", HelpForm::Description),
        "count - count operands\n"
    );
    assert_eq!(
        help.render("count", HelpForm::Full),
        "count: count [-n scale] [operand...]\n    count operands\n"
    );
}

#[test]
fn failures_convert_into_shell_errors_with_their_exit_codes() {
    let error = brush_core::Error::from(builtins::Failure::new("gone").with_exit_code(4));
    assert_eq!(error.to_string(), "gone");
    assert_eq!(u8::from(brush_core::ExecutionExitCode::from(&error)), 4);
    let error = brush_core::Error::from(builtins::Failure::new("gone"));
    assert_eq!(u8::from(brush_core::ExecutionExitCode::from(&error)), 1);
    assert!(UsageError::help_requested().is_help_request());
}

#[test]
fn failures_keep_the_error_they_were_made_from() {
    use std::error::Error as _;

    let failure = builtins::Failure::new(no_table("t"));
    assert_eq!(failure.to_string(), "t: no such table");
    assert_eq!(
        failure.downcast_ref::<NoTable>().map(|e| e.name.as_str()),
        Some("t")
    );
    // Its causes are the error's own.
    let cause = failure.source().expect("a cause");
    assert!(cause.downcast_ref::<std::io::Error>().is_some());
    // And they're reachable from the shell error it converts into.
    let error = brush_core::Error::from(failure);
    let mut causes = std::iter::successors(error.source(), |&e| e.source());
    assert!(causes.any(|e| e.is::<std::io::Error>()));

    // One made from a message displays as the message, and has no cause.
    let failure = builtins::Failure::new(format!("{}: not a function", "f"));
    assert_eq!(failure.to_string(), "f: not a function");
    assert!(failure.source().is_none());
    assert!(failure.downcast_ref::<NoTable>().is_none());
}
