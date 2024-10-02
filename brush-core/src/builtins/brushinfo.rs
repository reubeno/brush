use std::io::Write;

use clap::{Parser, Subcommand};

use crate::{builtins, commands, sys};

/// Change the current working directory.
#[derive(Parser)]
pub(crate) struct BrushInfoCommand {
    #[clap(subcommand)]
    command_group: CommandGroup,
}

#[derive(Subcommand)]
enum CommandGroup {
    #[clap(subcommand)]
    Process(ProcessCommand),
}

/// Commands for configuring tracing events.
#[expect(clippy::enum_variant_names)]
#[derive(Subcommand)]
enum ProcessCommand {
    /// Display process ID.
    #[clap(name = "pid")]
    ShowProcessId,
    /// Display process group ID.
    #[clap(name = "pgid")]
    ShowProcessGroupId,
    /// Display foreground process ID.
    #[clap(name = "fgpid")]
    ShowForegroundProcessId,
    /// Display parent process ID.
    #[clap(name = "ppid")]
    ShowParentProcessId,
}

#[async_trait::async_trait]
impl builtins::Command for BrushInfoCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        self.command_group.execute(&context)
    }
}

impl CommandGroup {
    fn execute(
        &self,
        context: &commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        match self {
            CommandGroup::Process(process) => process.execute(context),
        }
    }
}

impl ProcessCommand {
    fn execute(
        &self,
        context: &commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        match self {
            ProcessCommand::ShowProcessId => {
                writeln!(context.stdout(), "{}", std::process::id())?;
                Ok(builtins::ExitCode::Success)
            }
            ProcessCommand::ShowProcessGroupId => {
                if let Some(pgid) = sys::terminal::get_process_group_id() {
                    writeln!(context.stdout(), "{pgid}")?;
                    Ok(builtins::ExitCode::Success)
                } else {
                    writeln!(context.stderr(), "failed to get process group ID")?;
                    Ok(builtins::ExitCode::Custom(1))
                }
            }
            ProcessCommand::ShowForegroundProcessId => {
                if let Some(pid) = sys::terminal::get_foreground_pid() {
                    writeln!(context.stdout(), "{pid}")?;
                    Ok(builtins::ExitCode::Success)
                } else {
                    writeln!(context.stderr(), "failed to get foreground process ID")?;
                    Ok(builtins::ExitCode::Custom(1))
                }
            }
            ProcessCommand::ShowParentProcessId => {
                if let Some(pid) = sys::terminal::get_parent_process_id() {
                    writeln!(context.stdout(), "{pid}")?;
                    Ok(builtins::ExitCode::Success)
                } else {
                    writeln!(context.stderr(), "failed to get parent process ID")?;
                    Ok(builtins::ExitCode::Custom(1))
                }
            }
        }
    }
}
