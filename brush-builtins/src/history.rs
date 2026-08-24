use bpaf::Parser;
use std::ffi::OsStr;
use std::{
    io::Write,
    path::{Path, PathBuf},
};

use brush_core::{ExecutionExitCode, ExecutionResult, builtins, error, history};

/// Query or manipulate the shell's command history.
// TODO(history): Evaluate which of the options conflict with each other.
#[expect(clippy::option_option)]
pub(crate) struct HistoryCommand {
    /// Clears all history.
    clear_history: bool,

    /// Deletes the history entry at the given offset. Positive offsets are relative to the
    /// beginning of the history, while negative offsets are relative to the end of the history.
    delete_offset: Option<i64>,

    /// Appends the history from the current session to the history file.
    append_session_to_file: Option<Option<String>>,

    /// Appends any remaining history from the history file to the current session.
    append_rest_of_file_to_session: Option<Option<String>>,

    /// Appends the history from the history file to the current session.
    append_file_to_session: Option<Option<String>>,

    /// Replaces the history file with the current session history.
    write_session_to_file: Option<Option<String>>,

    /// History-expands positional arguments and displays them.
    expand_args: Option<Vec<String>>,

    /// Appends positional arguments as an entry in the current session.
    append_args_to_session: Option<Vec<String>>,

    /// Arguments.
    args: Vec<String>,
}

struct HistoryConfig {
    default_history_file_path: Option<PathBuf>,
    time_format: Option<String>,
}

impl builtins::Command for HistoryCommand {
    type Error = brush_core::Error;

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

    // N.B. Overrides the default [`builtins::Command::new`] so that flag-looking
    // values for `-d` and `-anrw` (e.g., `history -d -3`, a negative offset) get
    // joined into `-d=-3`; bpaf otherwise rejects separate flag-shaped values.
    fn new<I>(args: I) -> Result<Self, builtins::BuiltinArgParseError>
    where
        I: IntoIterator<Item = String>,
    {
        let mut args: Vec<String> = args.into_iter().collect();

        // N.B. The first argument is the command name itself.
        if !args.is_empty() {
            args.remove(0);
        }
        join_tokens_taking_values(&mut args, Self::value_taking_short_options());

        let (options, trailing) =
            builtins::split_option_section(&args, Self::value_taking_short_options(), &[]);

        let mut command = run_bpaf_parser::<Self>(&options)?;
        command.set_trailing_args(trailing);

        Ok(command)
    }

    fn set_trailing_args(&mut self, args: Vec<String>) {
        if self.append_args_to_session.is_some() {
            self.append_args_to_session = Some(args);
        } else {
            self.args = args;
        }
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        // Retrieve the shell's history config while we still can.
        let config = HistoryConfig {
            default_history_file_path: context.shell.history_file_path(),
            time_format: context.shell.history_time_format(),
        };

        let stdout = context.stdout();
        let stderr = context.stderr();

        if let Some(history) = context.shell.history_mut() {
            self.execute_with_history(history, &config, stdout, stderr)
        } else {
            Err(brush_core::ErrorKind::HistoryNotEnabled.into())
        }
    }
}

impl HistoryCommand {
    #[expect(clippy::cast_possible_wrap)]
    #[expect(clippy::cast_possible_truncation)]
    #[expect(clippy::cast_sign_loss)]
    fn execute_with_history(
        &self,
        history: &mut history::History,
        config: &HistoryConfig,
        stdout: impl Write,
        mut stderr: impl Write,
    ) -> Result<ExecutionResult, brush_core::Error> {
        if self.clear_history {
            history.clear()?;
        }

        if let Some(offset) = self.delete_offset {
            if offset == 0 {
                writeln!(stderr, "cannot delete history item at offset 0")?;
                return Ok(ExecutionExitCode::InvalidUsage.into());
            }

            if offset > 0 {
                // Convert to 0-based index.
                let index = (offset - 1) as usize;
                if !history.remove_nth_item(index) {
                    writeln!(stderr, "index past end of history")?;
                    return Ok(ExecutionExitCode::InvalidUsage.into());
                }
            } else {
                let count = history.count() as i64;
                let index = count + offset;
                if index < 0 {
                    writeln!(stderr, "index before beginning of history")?;
                    return Ok(ExecutionExitCode::InvalidUsage.into());
                }

                let _ = history.remove_nth_item(index as usize);
            }

            return Ok(ExecutionResult::success());
        }

        if let Some(append_option) = &self.append_session_to_file {
            if let Some(file_path) = get_effective_history_file_path(
                config.default_history_file_path.as_deref(),
                append_option.as_deref(),
            ) {
                history.flush(
                    file_path,
                    true,                         /* append? */
                    true,                         /* unsaved items only */
                    config.time_format.is_some(), /* write timestamps? */
                )?;
            }

            return Ok(ExecutionResult::success());
        }

        if self.append_rest_of_file_to_session.is_some() {
            return error::unimp("history -n is not yet implemented");
        }

        if self.append_file_to_session.is_some() {
            return error::unimp("history -r is not yet implemented");
        }

        if let Some(write_option) = &self.write_session_to_file {
            if let Some(file_path) = get_effective_history_file_path(
                config.default_history_file_path.as_deref(),
                write_option.as_deref(),
            ) {
                history.flush(
                    file_path,
                    false,                        /* append? */
                    false,                        /* unsaved items only? */
                    config.time_format.is_some(), /* write timestamps? */
                )?;
            }

            return Ok(ExecutionResult::success());
        }

        if self.expand_args.is_some() {
            return error::unimp("history -p is not yet implemented");
        }

        if let Some(args) = &self.append_args_to_session {
            history.add(history::Item::new(args.join(" ")))?;
            return Ok(ExecutionResult::success());
        }

        let max_entries: Option<usize> = if let Some(arg) = self.args.first() {
            Some(brush_core::int_utils::parse(arg.as_str(), 10)?)
        } else {
            None
        };

        display_history(history, config, max_entries, stdout, stderr)?;

        Ok(ExecutionResult::success())
    }
}

fn display_history(
    history: &history::History,
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

/// Builds a parser for one of the `-a`/`-n`/`-r`/`-w` options, each of which
/// takes an optional `HIST_FILE` value.
///
// N.B. Alternation of "with value" and "bare" forms wrapped in a final
// `optional` distinguishes between the option being absent and
// present-without-a-value.
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

fn run_bpaf_parser<T: builtins::Command>(
    args: &[String],
) -> Result<T, builtins::BuiltinArgParseError> {
    let os_args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    T::parser()
        .to_options()
        .run_inner(os_args.as_slice())
        .map_err(render_bpaf_failure)
}

fn render_bpaf_failure(failure: bpaf::ParseFailure) -> builtins::BuiltinArgParseError {
    match failure {
        bpaf::ParseFailure::Stdout(doc, full) => builtins::BuiltinArgParseError {
            message: doc.monochrome(full),
            help_request: true,
        },
        bpaf::ParseFailure::Completion(s) => builtins::BuiltinArgParseError {
            message: s,
            help_request: true,
        },
        bpaf::ParseFailure::Stderr(doc) => builtins::BuiltinArgParseError {
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

    fn new_from(args: &[&str]) -> Result<HistoryCommand, builtins::BuiltinArgParseError> {
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
