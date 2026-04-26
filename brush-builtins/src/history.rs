use brush_core::{ExecutionResult, builtins, error, history};
use clap::Parser;
use std::{io::Write, path::PathBuf};

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
    default_history_file_path: Option<PathBuf>,
    time_format: Option<String>,
}

impl builtins::Command for HistoryCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let config = HistoryConfig {
            default_history_file_path: context.shell.history_file_path(),
            time_format: context.shell.history_time_format(),
        };

        if let Some(history) = context.shell.history_mut() {
            let (output, stderr_output) = self.execute_with_history(history, &config)?;

            if !output.is_empty() {
                if let Some(mut stdout) = context.stdout() {
                    stdout.write_all(&output).await?;
                    stdout.flush().await?;
                }
            }

            if !stderr_output.is_empty() {
                if let Some(mut stderr) = context.stderr() {
                    stderr.write_all(&stderr_output).await?;
                    stderr.flush().await?;
                }
            }

            Ok(ExecutionResult::success())
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
    ) -> Result<(Vec<u8>, Vec<u8>), brush_core::Error> {
        let mut stderr_output = Vec::new();

        if self.clear_history {
            history.clear()?;
        }

        if let Some(offset) = self.delete_offset {
            if offset == 0 {
                writeln!(stderr_output, "cannot delete history item at offset 0")?;
                return Ok((Vec::new(), stderr_output));
            }

            if offset > 0 {
                let index = (offset - 1) as usize;
                if !history.remove_nth_item(index) {
                    writeln!(stderr_output, "index past end of history")?;
                }
            } else {
                let count = history.count() as i64;
                let index = count + offset;
                if index < 0 {
                    writeln!(stderr_output, "index before beginning of history")?;
                }

                let _ = history.remove_nth_item(index as usize);
            }

            return Ok((Vec::new(), stderr_output));
        }

        if let Some(append_option) = &self.append_session_to_file {
            if let Some(file_path) = get_effective_history_file_path(
                config.default_history_file_path.clone(),
                append_option.as_ref(),
            ) {
                history.flush(file_path, true, true, config.time_format.is_some())?;
            }

            return Ok((Vec::new(), Vec::new()));
        }

        if self.append_rest_of_file_to_session.is_some() {
            return error::unimp("history -n is not yet implemented");
        }

        if self.append_file_to_session.is_some() {
            return error::unimp("history -r is not yet implemented");
        }

        if let Some(write_option) = &self.write_session_to_file {
            if let Some(file_path) = get_effective_history_file_path(
                config.default_history_file_path.clone(),
                write_option.as_ref(),
            ) {
                history.flush(file_path, false, false, config.time_format.is_some())?;
            }

            return Ok((Vec::new(), Vec::new()));
        }

        if self.expand_args.is_some() {
            return error::unimp("history -p is not yet implemented");
        }

        if let Some(args) = &self.append_args_to_session {
            history.add(history::Item::new(args.join(" ")))?;
            return Ok((Vec::new(), Vec::new()));
        }

        let max_entries: Option<usize> = if let Some(arg) = self.args.first() {
            Some(brush_core::int_utils::parse(arg.as_str(), 10)?)
        } else {
            None
        };

        let output = display_history(history, config, max_entries)?;

        Ok((output, stderr_output))
    }
}

fn display_history(
    history: &history::History,
    config: &HistoryConfig,
    max_entries: Option<usize>,
) -> Result<Vec<u8>, brush_core::Error> {
    let mut output = Vec::new();
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

        std::writeln!(
            output,
            "{:>5}  {formatted_timestamp}{}",
            skip_count + i + 1,
            item.command_line
        )?;
    }

    Ok(output)
}

fn get_effective_history_file_path(
    default_history_file_path: Option<PathBuf>,
    option: Option<&String>,
) -> Option<PathBuf> {
    option.map_or_else(
        || default_history_file_path,
        |file_path| Some(PathBuf::from(file_path)),
    )
}
