use brush_core::{ExecutionExitCode, ExecutionResult, ResolvedPath, builtins, error, history};
use clap::Parser;
use std::io::Write;

/// Query or manipulate the shell's command history.
// TODO(history): Evaluate which of the options conflict with each other.
#[derive(Parser)]
#[expect(clippy::option_option)]
pub(crate) struct HistoryCommand {
    /// Clears all history.
    #[arg(short = 'c')]
    clear_history: bool,

    /// Deletes the history entry at the given offset. Positive offsets are relative to the
    /// beginning of the history, while negative offsets are relative to the end of the history.
    #[arg(short = 'd', value_name = "OFFSET")]
    delete_offset: Option<i64>,

    /// Appends the history from the current session to the history file.
    #[arg(short = 'a', group = "anrw", num_args = 0..=1, value_name = "HIST_FILE")]
    append_session_to_file: Option<Option<String>>,

    /// Appends any remaining history from the history file to the current session.
    #[arg(short = 'n', group = "anrw", num_args = 0..=1, value_name = "HIST_FILE")]
    append_rest_of_file_to_session: Option<Option<String>>,

    /// Appends the history from the history file to the current session.
    #[arg(short = 'r', group = "anrw", num_args = 0..=1, value_name = "HIST_FILE")]
    append_file_to_session: Option<Option<String>>,

    /// Replaces the history file with the current session history.
    #[arg(short = 'w', group = "anrw", num_args = 0..=1, value_name = "HIST_FILE")]
    write_session_to_file: Option<Option<String>>,

    /// History-expands positional arguments and displays them.
    #[arg(short = 'p', num_args = 0.., value_name = "ARG")]
    expand_args: Option<Vec<String>>,

    /// Appends positional arguments as an entry in the current session.
    #[arg(short = 's', num_args = 0.., value_name = "ARG")]
    append_args_to_session: Option<Vec<String>>,

    /// Arguments.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

struct HistoryConfig {
    /// The file that `-a` or `-w` acts on.
    history_file_path: Option<ResolvedPath>,
    time_format: Option<String>,
}

impl builtins::Command for HistoryCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let named_file = self
            .append_session_to_file
            .as_ref()
            .or(self.write_session_to_file.as_ref())
            .and_then(Option::as_deref);
        if named_file == Some("") {
            writeln!(context.stderr(), "{}: empty filename", context.command_name)?;
            return Ok(ExecutionResult::general_error());
        }

        // Retrieve the shell's history config while we still can.
        let config = HistoryConfig {
            history_file_path: named_file.map_or_else(
                || context.shell.history_file_path(),
                |file| Some(context.shell.absolute_path(file)),
            ),
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

        if self.append_session_to_file.is_some() {
            if let Some(file_path) = &config.history_file_path {
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

        if self.write_session_to_file.is_some() {
            if let Some(file_path) = &config.history_file_path {
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

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use pretty_assertions::{assert_eq, assert_matches};

    #[test]
    fn test_parse_dash_a() -> Result<()> {
        let cmd = HistoryCommand::try_parse_from(["history", "5"])?;
        assert_matches!(cmd.append_session_to_file, None);

        let cmd = HistoryCommand::try_parse_from(["history", "-a"])?;
        assert_matches!(cmd.append_session_to_file, Some(None));

        let cmd = HistoryCommand::try_parse_from(["history", "-a", "token"])?;
        assert_eq!(
            cmd.append_session_to_file,
            Some(Some(String::from("token")))
        );

        Ok(())
    }
}
