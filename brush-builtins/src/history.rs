//! The `history` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(HistoryCommand);

use brush_core::{ExecutionExitCode, ExecutionResult, error, history};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

pub(super) struct HistoryConfig {
    default_history_file_path: Option<PathBuf>,
    time_format: Option<String>,
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

pub(super) fn display_history(
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

pub(super) fn get_effective_history_file_path<'a>(
    default_history_file_path: Option<&'a Path>,
    option: Option<&'a str>,
) -> Option<&'a Path> {
    option.map(Path::new).or(default_history_file_path)
}

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &HistoryCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    // Retrieve the shell's history config while we still can.
    let config = HistoryConfig {
        default_history_file_path: context.shell.history_file_path(),
        time_format: context.shell.history_time_format(),
    };

    let stdout = context.stdout();
    let stderr = context.stderr();

    if let Some(history) = context.shell.history_mut() {
        command.execute_with_history(history, &config, stdout, stderr)
    } else {
        Err(brush_core::ErrorKind::HistoryNotEnabled.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use brush_core::args::FromArgs as _;
    use pretty_assertions::{assert_eq, assert_matches};

    // N.B. Parsed via the engine-agnostic `FromArgs` contract so this test
    // compiles (and runs) under whichever argument-parsing engine is selected.
    #[test]
    fn test_parse_dash_a() -> Result<()> {
        let cmd = HistoryCommand::from_args(&["history".into(), "5".into()])?;
        assert_matches!(cmd.append_session_to_file, None);

        let cmd = HistoryCommand::from_args(&["history".into(), "-a".into()])?;
        assert_matches!(cmd.append_session_to_file, Some(None));

        let cmd = HistoryCommand::from_args(&["history".into(), "-a".into(), "token".into()])?;
        assert_eq!(
            cmd.append_session_to_file,
            Some(Some(String::from("token")))
        );

        Ok(())
    }
}
