use brush_core::{ExecutionExitCode, ExecutionResult, builtins, error, history};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

/// Query or manipulate the shell's command history.
// TODO(history): Evaluate which of the options conflict with each other.
/// Bound to a bare `-a`, `-n`, `-r` or `-w`: the default history file.
const NO_FILE: &str = "\u{0}";

/// Display or manipulate the history list.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct HistoryCommand {
    /// Clear the history list.
    #[arg(short = 'c')]
    clear_history: bool,

    /// Delete the history entry at `offset` (negative: back from the end).
    #[arg(short = 'd', value_name = "offset", allow_negative_numbers)]
    delete_offset: Option<i64>,

    /// Append this session's history to the file.
    #[arg(short = 'a', value_name = "filename", default_missing = "\u{0}")]
    append_session_to_file: Option<String>,

    /// Read history lines not yet read from the file.
    #[arg(short = 'n', value_name = "filename", default_missing = "\u{0}")]
    append_rest_of_file_to_session: Option<String>,

    /// Read the file and append it to the history list.
    #[arg(short = 'r', value_name = "filename", default_missing = "\u{0}")]
    append_file_to_session: Option<String>,

    /// Write the history list to the file.
    #[arg(short = 'w', value_name = "filename", default_missing = "\u{0}")]
    write_session_to_file: Option<String>,

    /// Expand the arguments with history expansion and print them.
    #[arg(short = 'p')]
    expand: bool,

    /// Append the arguments to the history list as one entry.
    #[arg(short = 's')]
    append: bool,

    /// `n`, or with `-p`/`-s` the arguments.
    #[arg(
        positional,
        value_name = "arg",
        double_dash = "automatic",
        allow_negative_numbers
    )]
    args: Vec<String>,
}

struct HistoryConfig {
    default_history_file_path: Option<PathBuf>,
    time_format: Option<String>,
}

brush_builtin_winnow::winnow_builtin!(HistoryCommand);

impl builtins::Command for HistoryCommand {
    type Error = brush_core::Error;

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
                Some(append_option.as_str()).filter(|f| *f != NO_FILE),
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
                Some(write_option.as_str()).filter(|f| *f != NO_FILE),
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

        if self.expand {
            return error::unimp("history -p is not yet implemented");
        }

        if self.append {
            history.add(history::Item::new(self.args.join(" ")))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use brush_core::CommandArg;
    use brush_core::builtins::FromArgs;
    use pretty_assertions::{assert_eq, assert_matches};

    #[test]
    fn test_parse_dash_a() -> Result<()> {
        let cmd = HistoryCommand::from_args("history", vec![CommandArg::String("5".into())])?;
        assert_matches!(cmd.append_session_to_file, None);

        let cmd = HistoryCommand::from_args("history", vec![CommandArg::String("-a".into())])?;
        assert_eq!(cmd.append_session_to_file.as_deref(), Some(NO_FILE));

        let cmd = HistoryCommand::from_args(
            "history",
            vec![
                CommandArg::String("-a".into()),
                CommandArg::String("token".into()),
            ],
        )?;
        assert_eq!(cmd.append_session_to_file.as_deref(), Some("token"));

        Ok(())
    }
}
