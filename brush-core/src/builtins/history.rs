use crate::{builtins, commands, error, history, ExecutionParameters};
use clap::Parser;
use std::io::Write;

// TODO: Review which of these arguments override each other.
#[derive(Parser)]
pub(crate) struct HistoryCommand {
    /// Clears all history.
    #[arg(short = 'c')]
    clear_history: bool,

    /// Deletes the history entry at the given offset.
    #[arg(short = 'd')]
    delete_offset: Option<i64>,

    /// Appends the history from the current session to the history file.
    #[arg(short = 'a')]
    append_session_to_file: bool,

    /// Appends any remaining history from the history file to the current session.
    #[arg(short = 'n')]
    append_rest_of_file_to_session: bool,

    /// Appends the history from the history file to the current session.
    #[arg(short = 'r')]
    append_file_to_session: bool,

    /// Replaces the history file with the current session history.
    #[arg(short = 'w')]
    write_session_to_file: bool,

    /// History-expands positional arguments and displays them.
    #[arg(short = 'p')]
    expand_args: bool,

    /// Appends positional arguments as an entry in the current session.
    #[arg(short = 's')]
    append_args_to_session: bool,

    /// Arguments.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for HistoryCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<builtins::ExitCode, error::Error> {
        if let Some(history) = context.shell.history_mut() {
            self.execute_with_history(&context.params, history)
        } else {
            writeln!(
                context.stderr(),
                "history: history not available in this shell"
            )?;
            Ok(builtins::ExitCode::Unimplemented)
        }
    }
}

impl HistoryCommand {
    fn execute_with_history(
        &self,
        params: &ExecutionParameters,
        history: &mut history::History,
    ) -> Result<builtins::ExitCode, error::Error> {
        if self.clear_history {
            history.clear()?;
        }

        if self.delete_offset.is_some() {
            return error::unimp("history -d is not yet implemented");
        }

        if self.append_session_to_file {
            return error::unimp("history -a is not yet implemented");
        }

        if self.append_rest_of_file_to_session {
            return error::unimp("history -n is not yet implemented");
        }

        if self.append_file_to_session {
            return error::unimp("history -r is not yet implemented");
        }

        if self.write_session_to_file {
            history.flush()?;
        }

        if self.expand_args {
            return error::unimp("history -p is not yet implemented");
        }

        if self.append_args_to_session {
            return error::unimp("history -n is not yet implemented");
        }

        // If we got down here, then no options were specified. If there's a positional
        // argument, then it should be a number, indicating how many entries to display.
        if self.args.len() > 1 {
            writeln!(params.stderr(), "history: too many arguments")?;
            return Ok(builtins::ExitCode::InvalidUsage);
        }

        let max_entries: Option<usize> = if let Some(arg) = self.args.first() {
            Some(arg.parse()?)
        } else {
            None
        };

        display_history(params, history, max_entries)?;

        Ok(builtins::ExitCode::Success)
    }
}

fn display_history(
    params: &ExecutionParameters,
    history: &mut history::History,
    max_entries: Option<usize>,
) -> Result<(), error::Error> {
    let item_count = history.count();
    let skip_count = item_count - max_entries.unwrap_or(item_count);

    for (i, item) in history.iter().skip(skip_count).enumerate() {
        // Output format is something like:
        //     1  echo hello world
        std::writeln!(
            params.stdout(),
            "{:>5}  {}",
            skip_count + i + 1,
            item.command_line
        )?;
    }

    Ok(())
}
