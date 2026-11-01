use brush_core::{ExecutionExitCode, ExecutionResult, builtins, error, history};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

/// Query or manipulate the shell's command history.
// TODO(history): Evaluate which of the options conflict with each other.
#[derive(winnow_args::Args)]
#[arg(
    group("anrw", message = "cannot use more than one of -anrw"),
    disable_help_short,
    disable_version_flag,
    disable_help_subcommand
)]
pub(crate) struct HistoryCommand {
    /// Clears all history.
    #[arg(short = 'c')]
    clear_history: bool,

    /// Deletes the history entry at the given offset. Positive offsets are relative to the
    /// beginning of the history, while negative offsets are relative to the end of the history.
    #[arg(short = 'd', value_name = "OFFSET", allow_negative_numbers)]
    delete_offset: Option<i64>,

    /// Appends the history from the current session to the history file. The
    /// first argument names the file, if given.
    #[arg(short = 'a', group = "anrw")]
    append_session_to_file: bool,

    /// Appends any remaining history from the history file to the current session. The
    /// first argument names the file, if given.
    #[arg(short = 'n', group = "anrw")]
    append_rest_of_file_to_session: bool,

    /// Appends the history from the history file to the current session. The first
    /// argument names the file, if given.
    #[arg(short = 'r', group = "anrw")]
    append_file_to_session: bool,

    /// Replaces the history file with the current session history. The first
    /// argument names the file, if given.
    #[arg(short = 'w', group = "anrw")]
    write_session_to_file: bool,

    /// History-expands positional arguments and displays them.
    #[arg(short = 'p')]
    expand: bool,

    /// Appends positional arguments as an entry in the current session.
    #[arg(short = 's')]
    append: bool,

    /// Arguments.
    #[arg(positional, double_dash = "automatic", allow_negative_numbers)]
    args: Vec<String>,
}

struct HistoryConfig {
    default_history_file_path: Option<PathBuf>,
    time_format: Option<String>,
}

brush_winnow_args_adapter::impl_parse_args!(HistoryCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for HistoryCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        // Retrieve the shell's history config while we still can.
        let config = HistoryConfig {
            default_history_file_path: context.shell.history_file_path(),
            time_format: context.shell.history_time_format(),
        };

        let stderr = context.stderr();
        let mut output = Vec::new();

        let result = if let Some(history) = context.shell.history_mut() {
            self.execute_with_history(history, &config, &mut output, stderr)
        } else {
            Err(brush_core::ErrorKind::HistoryNotEnabled.into())
        };

        if !output.is_empty() {
            if let Some(mut stdout) = context.stdout_async() {
                stdout.write_all(&output).await?;
                stdout.flush().await?;
            } else {
                context.stdout().write_all(&output)?;
                context.stdout().flush()?;
            }
        }

        result
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

        if self.append_session_to_file {
            if let Some(file_path) = self.history_file_path(config) {
                history.flush(
                    file_path,
                    true,                         /* append? */
                    true,                         /* unsaved items only */
                    config.time_format.is_some(), /* write timestamps? */
                )?;
            }

            return Ok(ExecutionResult::success());
        }

        if self.append_rest_of_file_to_session {
            return error::unimp("history -n is not yet implemented");
        }

        if self.append_file_to_session {
            return error::unimp("history -r is not yet implemented");
        }

        if self.write_session_to_file {
            if let Some(file_path) = self.history_file_path(config) {
                history.flush(
                    file_path,
                    false,                        /* append? */
                    false,                        /* unsaved items only? */
                    config.time_format.is_some(), /* write timestamps? */
                )?;
            }

            return Ok(ExecutionResult::success());
        }

        // With -s, -p only expands the entry; history expansion isn't implemented.
        if self.append {
            history.add(history::Item::new(self.args.join(" ")))?;
            return Ok(ExecutionResult::success());
        }

        if self.expand {
            return error::unimp("history -p is not yet implemented");
        }

        let max_entries: Option<usize> = if let Some(arg) = self.args.first() {
            Some(brush_core::int_utils::parse(arg.as_str(), 10)?)
        } else {
            None
        };

        display_history(history, config, max_entries, stdout, stderr)?;

        Ok(ExecutionResult::success())
    }

    /// The file named by the first argument, or else the shell's history file.
    fn history_file_path<'a>(&'a self, config: &'a HistoryConfig) -> Option<&'a Path> {
        self.args
            .first()
            .map(Path::new)
            .or(config.default_history_file_path.as_deref())
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

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use anyhow::Result;
    use brush_core::builtins::{Invocation, ParseArgs};
    use pretty_assertions::assert_eq;

    fn parse(words: &[&str]) -> Result<HistoryCommand> {
        Ok(<HistoryCommand as ParseArgs>::parse(Invocation::new(
            "history",
            words.iter().copied(),
        ))?)
    }

    #[test]
    fn test_parse_dash_a() -> Result<()> {
        let config = HistoryConfig {
            default_history_file_path: Some(PathBuf::from("default")),
            time_format: None,
        };

        let cmd = parse(&["5"])?;
        assert!(!cmd.append_session_to_file);

        let cmd = parse(&["-a"])?;
        assert!(cmd.append_session_to_file);
        assert_eq!(cmd.history_file_path(&config), Some(Path::new("default")));

        for words in [
            &["-a", "token"][..],
            &["-a", "--", "token"],
            &["-ac", "token"],
        ] {
            let cmd = parse(words)?;
            assert!(cmd.append_session_to_file);
            assert_eq!(cmd.history_file_path(&config), Some(Path::new("token")));
        }

        assert!(parse(&["-atoken"]).is_err());

        Ok(())
    }
}
