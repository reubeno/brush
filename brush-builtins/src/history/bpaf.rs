//! `history` builtin: `HistoryCommand` instrumented for bpaf.

#![cfg(feature = "parser-bpaf")]

// N.B. Some transplanted helpers await wiring during migration.
#![allow(dead_code, reason = "transitional engine scaffolding")]

use bpaf::Parser;
use brush_core::args::{ArgsError, FromArgs};
use brush_core::builtins;
use std::path::{Path, PathBuf};
use std::io::Write;

struct HistoryConfig {
    default_history_file_path: Option<PathBuf>,
    time_format: Option<String>,
}

fn display_history(
    history: &brush_core::history::History,
    config: &HistoryConfig,
    max_entries: Option<usize>,
    mut stdout: impl Write,
    _stderr: impl Write,
) -> Result<(), brush_core::Error> {
    let item_count = history.count();
    let skip_count = item_count - max_entries.unwrap_or(item_count);

    for (i, item) in history.iter().skip(skip_count).enumerate() {
        let mut formatted_timestamp = String::new();

        if let Some(timestamp) = item.timestamp {
            let local_timestamp = timestamp.with_timezone(&chrono::Local);
            if let Some(time_format) = &config.time_format {
                let fmt_items = chrono::format::StrftimeItems::new(time_format);
                formatted_timestamp = local_timestamp.format_with_items(fmt_items).to_string();
            }
        }

        // Output format is something like:
        //     1  echo hello world
        std::writeln!(
            stdout,
            "{:>5}  {formatted_timestamp}{}",
            skip_count + i + 1,
            item.command_line
        )?;
    }

    Ok(())
}

fn get_effective_history_file_path<'a>(
    default_history_file_path: Option<&'a Path>,
    option: Option<&'a str>,
) -> Option<&'a Path> {
    option.map(Path::new).or(default_history_file_path)
}

/// Merges `-X` tokens followed by a flag-looking value token into `-X=<value>`
/// so that bpaf accepts values that would otherwise be rejected as flags;
/// e.g., negative offsets.
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

fn hist_file_option(
    short_char: char,
    help: &'static str,
) -> impl bpaf::Parser<Option<Option<String>>> {
    let with_value = bpaf::short(short_char)
        .help(help)
        .argument::<String>("HIST_FILE")
        .map(Some);
    let bare = bpaf::short(short_char).req_flag(()).map(|()| None);

    bpaf::construct!([with_value, bare]).optional()
}

fn run_bpaf_parser<T: crate::args::bpaf_support::BpafArgs>(args: &[String]) -> Result<T, ArgsError> {
    crate::args::bpaf_support::run_parser::<T>(args)
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
mod tests {
    use super::*;
    use anyhow::Result;
    use brush_core::builtins::Command as _;
    use pretty_assertions::{assert_eq, assert_matches};

    fn new_from(args: &[&str]) -> Result<HistoryCommand, ArgsError> {
        HistoryCommand::new(
            std::iter::once("history".to_string()).chain(args.iter().map(|s| s.to_string())),
        )
    }

    #[test]
    fn test_parse_dash_a() -> Result<()> {
        assert_matches!(new_from(&["5"])?.append_session_to_file, None);

        assert_matches!(new_from(&["-a"])?.append_session_to_file, Some(None));

        assert_eq!(
            new_from(&["-a", "token"])?.append_session_to_file,
            Some(Some(String::from("token")))
        );

        Ok(())
    }

    #[test]
    fn test_parse_negative_delete_offset() -> Result<()> {
        assert_eq!(new_from(&["-d", "-3"])?.delete_offset, Some(-3));

        Ok(())
    }

    #[test]
    fn test_parse_append_args_to_session() -> Result<()> {
        let cmd = new_from(&["-s", "echo", "hello", "world"])?;
        assert_matches!(cmd.append_args_to_session, Some(_));
        assert_eq!(
            cmd.append_args_to_session.unwrap(),
            ["echo", "hello", "world"]
        );

        Ok(())
    }

    #[test]
    fn test_parse_max_entries() -> Result<()> {
        let cmd = new_from(&["5"])?;
        assert_eq!(cmd.args, ["5"]);

        Ok(())
    }
}

#[expect(clippy::option_option)]
pub(crate) struct HistoryCommand {
    /// Clears all history.
    pub(super) clear_history: bool,

    /// Deletes the history entry at the given offset. Positive offsets are relative to the
    /// beginning of the history, while negative offsets are relative to the end of the history.
    pub(super) delete_offset: Option<i64>,

    /// Appends the history from the current session to the history file.
    pub(super) append_session_to_file: Option<Option<String>>,

    /// Appends any remaining history from the history file to the current session.
    pub(super) append_rest_of_file_to_session: Option<Option<String>>,

    /// Appends the history from the history file to the current session.
    pub(super) append_file_to_session: Option<Option<String>>,

    /// Replaces the history file with the current session history.
    pub(super) write_session_to_file: Option<Option<String>>,

    /// History-expands positional arguments and displays them.
    pub(super) expand_args: Option<Vec<String>>,

    /// Appends positional arguments as an entry in the current session.
    pub(super) append_args_to_session: Option<Vec<String>>,

    /// Arguments.
    pub(super) args: Vec<String>,
}

impl crate::args::bpaf_support::BpafArgs for HistoryCommand {
fn parser() -> impl bpaf::Parser<Self> {
        // N.B. Only the leading options are parsed here; all remaining tokens
        // are captured verbatim via `takes_trailing_args`.
        let clear_history = bpaf::short('c').help("Clears all history.").switch();
        let delete_offset = bpaf::short('d')
            .help(
                "Deletes the history entry at the given offset. Positive offsets are \
                 relative to the beginning of the history, while negative offsets are \
                 relative to the end of the history.",
            )
            .argument::<i64>("OFFSET")
            .optional();

        let append_session_to_file = hist_file_option(
            'a',
            "Appends the history from the current session to the history file.",
        );
        let append_rest_of_file_to_session = hist_file_option(
            'n',
            "Appends any remaining history from the history file to the current session.",
        );
        let append_file_to_session = hist_file_option(
            'r',
            "Appends the history from the history file to the current session.",
        );
        let write_session_to_file = hist_file_option(
            'w',
            "Replaces the history file with the current session history.",
        );
        let expand_args = bpaf::short('p')
            .help("History-expands positional arguments and displays them.")
            .switch()
            .map(|present| present.then(Vec::new));
        let append_args_to_session = bpaf::short('s')
            .help("Appends positional arguments as an entry in the current session.")
            .switch()
            .map(|present| present.then(Vec::new));
        let args = bpaf::pure(Vec::new());

        bpaf::construct!(HistoryCommand {
            clear_history,
            delete_offset,
            append_session_to_file,
            append_rest_of_file_to_session,
            append_file_to_session,
            write_session_to_file,
            expand_args,
            append_args_to_session,
            args,
        })
    }
fn about() -> &'static str {
        "Query or manipulate the shell's command history."
    }
fn synopsis() -> &'static str {
        "[-c] [-d OFFSET] [-anrw] [-ps] [ARGS]..."
    }
fn takes_trailing_args() -> bool {
        true
    }
fn value_taking_short_options() -> &'static str {
        "danrw"
    }
fn set_trailing_args(&mut self, args: Vec<String>) {
        if self.append_args_to_session.is_some() {
            self.append_args_to_session = Some(args);
        } else {
            self.args = args;
        }
    }
    fn from_words(words: &[String]) -> Result<Self, ArgsError> {
        let args = words.to_vec();

        let mut args: Vec<String> = args.into_iter().collect();

        // N.B. The first argument is the command name itself.
        if !args.is_empty() {
            args.remove(0);
        }
        join_tokens_taking_values(&mut args, Self::value_taking_short_options());

        let (options, trailing) =
            crate::args::bpaf_support::split_option_section(&args, Self::value_taking_short_options(), &[]);

        let mut command = run_bpaf_parser::<Self>(&options)?;
        command.set_trailing_args(trailing);

        Ok(command)
    
    }
}

impl FromArgs for HistoryCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        crate::args::bpaf_support::BpafArgs::from_words(words)
    }
}

impl builtins::Command for HistoryCommand {
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
