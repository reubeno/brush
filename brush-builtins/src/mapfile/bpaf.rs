//! `mapfile` builtin: `MapFileCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Parser;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;

fn setup_terminal_settings(
    file: &brush_core::openfiles::OpenFile,
) -> Result<Option<brush_core::terminal::AutoModeGuard>, brush_core::Error> {
    let mode = brush_core::terminal::AutoModeGuard::new(file.to_owned()).ok();
    if let Some(mode) = &mode {
        let config = brush_core::terminal::Settings::builder()
            .line_input(false)
            .interrupt_signals(false)
            .build();

        mode.apply_settings(&config)?;
    }

    Ok(mode)
}

/// Merges `-X` tokens followed by a flag-looking value token into `-X=<value>`
/// so that bpaf accepts values that would otherwise be rejected as flags;
/// e.g., negative numbers.
fn join_tokens_taking_values(args: &mut Vec<String>, shorts: &str) {
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].clone();

        if arg == "--" {
            break;
        }

        let takes_value = arg.len() == 2
            && arg.starts_with('-')
            && arg.chars().nth(1).is_some_and(|c| shorts.contains(c));

        if takes_value {
            if let Some(next) = args.get(i + 1) {
                if next.starts_with('-') && next != "-" && next != "--" {
                    args[i] = format!("{arg}={next}");
                    args.remove(i + 1);
                }
            }
        }

        i += 1;
    }
}

fn run_bpaf_parser<T: crate::args::bpaf_support::BpafArgs>(args: &[String]) -> Result<T, ArgsError> {
    crate::args::run_parser::<T>(args)
}

fn render_bpaf_failure(failure: bpaf::ParseFailure) -> ArgsError {
    match failure {
        bpaf::ParseFailure::Stdout(doc, full) => ArgsError {
            message: doc.monochrome(full),
            help_request: true,
        },
        bpaf::ParseFailure::Completion(s) => ArgsError {
            message: s,
            help_request: true,
        },
        bpaf::ParseFailure::Stderr(doc) => ArgsError {
            message: doc.monochrome(true),
            help_request: false,
        },
    }
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use brush_core::builtins::Command as _;

    fn new_from(args: &[&str]) -> Result<MapFileCommand, ArgsError> {
        MapFileCommand::new(
            std::iter::once("mapfile".to_string()).chain(args.iter().map(|s| s.to_string())),
        )
    }

    #[test]
    fn test_defaults() -> anyhow::Result<()> {
        let cmd = new_from(&[])?;
        assert_eq!(cmd.max_count, 0);
        assert_eq!(cmd.skip_count, 0);
        assert_eq!(cmd.fd, 0);
        assert_eq!(cmd.callback_group_size, 5000);
        assert_eq!(cmd.array_var_name, "MAPFILE");
        assert_eq!(cmd.origin, None);
        Ok(())
    }

    #[test]
    fn test_negative_origin_separate_token() -> anyhow::Result<()> {
        let cmd = new_from(&["-O", "-3"])?;
        assert_eq!(cmd.origin, Some(-3));
        Ok(())
    }

    #[test]
    fn test_options_with_array_name() -> anyhow::Result<()> {
        let cmd = new_from(&["-t", "-u", "1", "-s", "2", "-n", "10", "myarray"])?;
        assert!(cmd.remove_delimiter);
        assert_eq!(cmd.fd, 1);
        assert_eq!(cmd.skip_count, 2);
        assert_eq!(cmd.max_count, 10);
        assert_eq!(cmd.array_var_name, "myarray");
        Ok(())
    }

    #[test]
    fn test_invalid_skip_count_rejected() {
        assert!(new_from(&["-s", "-1"]).is_err());
    }
}

/// Read lines from standard input into an indexed array variable.
pub(crate) struct MapFileCommand {
    /// Delimiter to use (defaults to newline).
    pub(super) delimiter: Option<String>,

    /// Maximum number of entries to read (0 means no limit).
    pub(super) max_count: i64,

    /// Index into array at which to start assignment.
    pub(super) origin: Option<i64>,

    /// Number of initial entries to skip.
    pub(super) skip_count: i64,

    /// Whether or not to remove the delimiter from each read line.
    pub(super) remove_delimiter: bool,

    /// File descriptor to read from (defaults to stdin).
    pub(super) fd: brush_core::ShellFd,

    /// Name of function to call for each group of lines.
    pub(super) callback: Option<String>,

    /// Number of lines to pass the callback for each group.
    pub(super) callback_group_size: i64,

    /// Name of array to read into.
    pub(super) array_var_name: String,
}

impl crate::args::bpaf_support::BpafArgs for MapFileCommand {
fn parser() -> impl bpaf::Parser<Self> {
        let delimiter = bpaf::short('d')
            .help("Delimiter to use (defaults to newline).")
            .argument::<String>("DELIM")
            .optional();
        let max_count = bpaf::short('n')
            .help("Maximum number of entries to read (0 means no limit).")
            .argument::<i64>("COUNT")
            .fallback(0);
        let origin = bpaf::short('O')
            .help("Index into array at which to start assignment.")
            .argument::<i64>("ORIGIN")
            .optional();
        let skip_count = bpaf::short('s')
            .help("Number of initial entries to skip.")
            .argument::<i64>("COUNT")
            .guard(|v| *v >= 0, "must be >= 0")
            .fallback(0);
        let remove_delimiter = bpaf::short('t')
            .help("Whether or not to remove the delimiter from each read line.")
            .switch();
        let fd = bpaf::short('u')
            .help("File descriptor to read from (defaults to stdin).")
            .argument::<brush_core::ShellFd>("FD")
            .fallback(0);
        let callback = bpaf::short('C')
            .help("Name of function to call for each group of lines.")
            .argument::<String>("CALLBACK")
            .optional();
        let callback_group_size = bpaf::short('c')
            .help("Number of lines to pass the callback for each group.")
            .argument::<i64>("COUNT")
            .guard(|v| *v >= 1, "must be >= 1")
            .fallback(5000);
        let array_var_name = bpaf::positional::<String>("ARRAY_VAR_NAME")
            .help("Name of array to read into.")
            .fallback(String::from("MAPFILE"));

        bpaf::construct!(MapFileCommand {
            delimiter,
            max_count,
            origin,
            skip_count,
            remove_delimiter,
            fd,
            callback,
            callback_group_size,
            array_var_name,
        })
    }
fn about() -> &'static str {
        "Read lines from standard input into an indexed array variable."
    }
fn synopsis() -> &'static str {
        "[-d DELIM] [-n COUNT] [-O ORIGIN] [-s COUNT] [-t] [-u FD] [-C CALLBACK] [-c COUNT] [ARRAY_VAR_NAME]"
    }
    fn from_words(words: &[String]) -> Result<Self, ArgsError> {
        let args = words.to_vec();

        let mut args: Vec<String> = args.into_iter().collect();

        // N.B. The first argument is the command name itself.
        if !args.is_empty() {
            args.remove(0);
        }
        join_tokens_taking_values(&mut args, "O");

        run_bpaf_parser::<Self>(&args)
    
    }
}

impl FromArgs for MapFileCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for MapFileCommand {
    type Error = brush_core::Error;

    fn get_content(
        name: &str,
        content_type: builtins::ContentType,
        options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::error::Error> {
        crate::args::bpaf_support::get_content::<Self>(name, &content_type, options)
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        super::execute(self, context).await
    }
}
