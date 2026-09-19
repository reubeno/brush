//! Proves the builtin contracts are engine-neutral: these builtins are parsed
//! and documented entirely by hand, with no `clap` type anywhere in them, and
//! the shell runs them like any other.
//!
//! `brush-core` has no dependency on clap or any other parsing engine, so this is
//! the only way a builtin can be written from within this crate's tests. If the
//! seam ever grows an engine-specific requirement, this file stops compiling.

#![cfg(test)]
#![allow(
    clippy::panic_in_result_fn,
    clippy::expect_used,
    clippy::unused_async_trait_impl
)]

use anyhow::Result;
use brush_core::{CommandArg, ExecutionResult, builtins};

type SE = brush_core::extensions::DefaultShellExtensions;

/// Reports how many operands it was given, optionally scaled by `-n`, as its
/// exit code. Contrived, but it exercises an option with a value, an operand
/// list, and a usage error without needing to capture output.
struct CountCommand {
    scale: u8,
    operands: usize,
}

impl builtins::FromArgs for CountCommand {
    fn from_args(name: &str, args: Vec<CommandArg>) -> Result<Self, builtins::ArgsError> {
        let mut scale = 1;
        let mut operands = 0;
        let mut words = args.iter().map(ToString::to_string);

        while let Some(word) = words.next() {
            match word.as_str() {
                "--help" => {
                    return Err(builtins::ArgsError::HelpRequested(format!(
                        "{name}: {}",
                        <Self as builtins::HelpContent>::synopsis(name)
                    )));
                }
                "-n" => {
                    let value = words
                        .next()
                        .ok_or_else(|| usage(name, "option requires an argument -- n"))?;
                    scale = value
                        .parse()
                        .map_err(|_| usage(name, "numeric argument required"))?;
                }
                _ if word.starts_with('-') && word.len() > 1 => {
                    return Err(usage(name, &format!("{word}: invalid option")));
                }
                _ => operands += 1,
            }
        }

        Ok(Self { scale, operands })
    }
}

fn usage(name: &str, message: &str) -> builtins::ArgsError {
    builtins::ArgsError::Usage(format!("{name}: {message}"))
}

impl builtins::HelpContent for CountCommand {
    fn synopsis(name: &str) -> String {
        format!("{name} [-n scale] [operand...]")
    }

    fn description(_name: &str) -> String {
        "count operands".to_owned()
    }
}

impl builtins::Command for CountCommand {
    type Error = brush_core::Error;

    async fn execute<S: brush_core::ShellExtensions>(
        &self,
        _context: brush_core::ExecutionContext<'_, S>,
    ) -> Result<ExecutionResult, Self::Error> {
        #[expect(clippy::cast_possible_truncation)]
        Ok(ExecutionResult::new(self.operands as u8 * self.scale))
    }
}

/// Takes declarations, and reports what it received as its exit code: tens for
/// assignments, ones for strings. It does no option handling of its own, so any
/// splitting or dropping of dash-prefixed words would have to be the shell's.
struct DeclCountCommand {
    assignments: u8,
    strings: u8,
}

impl builtins::FromArgs for DeclCountCommand {
    const TAKES_DECLARATIONS: bool = true;

    fn from_args(_name: &str, args: Vec<CommandArg>) -> Result<Self, builtins::ArgsError> {
        let mut this = Self {
            assignments: 0,
            strings: 0,
        };

        for arg in args {
            match arg {
                CommandArg::Assignment(_) => this.assignments += 1,
                CommandArg::String(_) => this.strings += 1,
            }
        }

        Ok(this)
    }
}

impl builtins::HelpContent for DeclCountCommand {
    fn synopsis(name: &str) -> String {
        format!("{name} [arg...]")
    }

    fn description(_name: &str) -> String {
        "count declarations".to_owned()
    }
}

impl builtins::Command for DeclCountCommand {
    type Error = brush_core::Error;

    async fn execute<S: brush_core::ShellExtensions>(
        &self,
        _context: brush_core::ExecutionContext<'_, S>,
    ) -> Result<ExecutionResult, Self::Error> {
        Ok(ExecutionResult::new(self.assignments * 10 + self.strings))
    }
}

async fn exit_code_of(command: &str) -> Result<u8> {
    let mut shell = brush_core::Shell::builder()
        .builtin("count", builtins::builtin::<CountCommand, SE>())
        .builtin("declcount", builtins::builtin::<DeclCountCommand, SE>())
        .build()
        .await?;

    let params = shell.default_exec_params();
    let result = shell
        .run_string(command, &brush_core::SourceInfo::default(), &params)
        .await?;

    Ok(u8::from(result.exit_code))
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
async fn hand_parsed_builtin_reports_usage_errors() -> Result<()> {
    assert_eq!(exit_code_of("count -z").await?, 2);
    assert_eq!(exit_code_of("count -n").await?, 2);
    Ok(())
}

#[tokio::test]
async fn hand_parsed_builtin_help_request_exits_with_usage_status() -> Result<()> {
    // A help request is reported like a usage error: on stderr, with the usage
    // status.
    assert_eq!(exit_code_of("count --help").await?, 2);
    Ok(())
}

#[tokio::test]
async fn shell_passes_arguments_through_without_option_handling() -> Result<()> {
    // Two assignments and three strings, including dash-prefixed words both
    // before and after the assignments: nothing is split off, dropped, or
    // flattened on the builtin's behalf.
    assert_eq!(exit_code_of("declcount -x a=1 -r b=2 c").await?, 23);
    Ok(())
}

#[tokio::test]
async fn assignments_stay_strings_unless_the_builtin_takes_declarations() -> Result<()> {
    // `count` doesn't take declarations, so `a=1` reaches it as a plain operand.
    assert_eq!(exit_code_of("count a=1 b").await?, 2);
    Ok(())
}

#[test]
fn shell_frames_short_help_forms() -> Result<()> {
    let registration = builtins::builtin::<CountCommand, SE>();
    let render = |content_type| {
        (registration.content_func())("count", content_type, &builtins::ContentOptions::default())
    };

    assert_eq!(
        render(builtins::ContentType::ShortUsage)?,
        "count: count [-n scale] [operand...]\n"
    );
    assert_eq!(
        render(builtins::ContentType::ShortDescription)?,
        "count - count operands\n"
    );
    assert_eq!(
        render(builtins::ContentType::DetailedHelp)?,
        "count: count [-n scale] [operand...]\n    count operands\n"
    );
    Ok(())
}
