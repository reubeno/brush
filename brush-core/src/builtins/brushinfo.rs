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
    #[clap(subcommand)]
    Complete(CompleteCommand),
}

/// Commands for configuring tracing events.
#[allow(clippy::enum_variant_names)]
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

/// Commands for generating completions.
#[derive(Subcommand)]
enum CompleteCommand {
    /// Generate completions for an input line.
    #[clap(name = "line")]
    Line {
        /// The 0-indexed cursor position for generation.
        #[arg(long = "cursor", short = 'c')]
        cursor_index: Option<usize>,

        /// The input line to generate completions for.
        line: String,
    },
}

impl builtins::Command for BrushInfoCommand {
    async fn execute(
        &self,
        mut context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        self.command_group.execute(&mut context).await
    }
}

impl CommandGroup {
    async fn execute(
        &self,
        context: &mut commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        match self {
            Self::Process(process) => process.execute(context),
            Self::Complete(complete) => complete.execute(context).await,
        }
    }
}

impl ProcessCommand {
    fn execute(
        &self,
        context: &commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        match self {
            Self::ShowProcessId => {
                writeln!(context.stdout(), "{}", std::process::id())?;
                Ok(builtins::ExitCode::Success)
            }
            Self::ShowProcessGroupId => {
                if let Some(pgid) = sys::terminal::get_process_group_id() {
                    writeln!(context.stdout(), "{pgid}")?;
                    Ok(builtins::ExitCode::Success)
                } else {
                    writeln!(context.stderr(), "failed to get process group ID")?;
                    Ok(builtins::ExitCode::Custom(1))
                }
            }
            Self::ShowForegroundProcessId => {
                if let Some(pid) = sys::terminal::get_foreground_pid() {
                    writeln!(context.stdout(), "{pid}")?;
                    Ok(builtins::ExitCode::Success)
                } else {
                    writeln!(context.stderr(), "failed to get foreground process ID")?;
                    Ok(builtins::ExitCode::Custom(1))
                }
            }
            Self::ShowParentProcessId => {
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

impl CompleteCommand {
    async fn execute(
        &self,
        context: &mut commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        match self {
            Self::Line { cursor_index, line } => {
                let completions = context
                    .shell
                    .get_completions(line, cursor_index.unwrap_or(line.len()))
                    .await?;
                for candidate in completions.candidates {
                    writeln!(context.stdout(), "{candidate}")?;
                }
                Ok(builtins::ExitCode::Success)
            }
        }
    }
}
