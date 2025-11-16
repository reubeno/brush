use std::io::Write;

use clap::{Parser, Subcommand};

use brush_core::{ExecutionResult, builtins, sys};

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
    type Error = brush_core::Error;

    async fn execute(
        &self,
        mut context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        self.command_group.execute(&mut context).await
    }
}

impl CommandGroup {
    async fn execute(
        &self,
        context: &mut brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::ExecutionResult, brush_core::Error> {
        match self {
            Self::Process(process) => process.execute(context),
            Self::Complete(complete) => complete.execute(context).await,
        }
    }
}

impl ProcessCommand {
    fn execute(
        &self,
        context: &brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::ExecutionResult, brush_core::Error> {
        match self {
            Self::ShowProcessId => {
                writeln!(context.stdout(), "{}", std::process::id())?;
                Ok(ExecutionResult::success())
            }
            Self::ShowProcessGroupId => {
                if let Some(pgid) = sys::terminal::get_process_group_id() {
                    writeln!(context.stdout(), "{pgid}")?;
                    Ok(ExecutionResult::success())
                } else {
                    writeln!(context.stderr(), "failed to get process group ID")?;
                    Ok(ExecutionResult::general_error())
                }
            }
            Self::ShowForegroundProcessId => {
                if let Some(pid) = sys::terminal::get_foreground_pid() {
                    writeln!(context.stdout(), "{pid}")?;
                    Ok(ExecutionResult::success())
                } else {
                    writeln!(context.stderr(), "failed to get foreground process ID")?;
                    Ok(ExecutionResult::general_error())
                }
            }
            Self::ShowParentProcessId => {
                if let Some(pid) = sys::terminal::get_parent_process_id() {
                    writeln!(context.stdout(), "{pid}")?;
                    Ok(ExecutionResult::success())
                } else {
                    writeln!(context.stderr(), "failed to get parent process ID")?;
                    Ok(ExecutionResult::general_error())
                }
            }
        }
    }
}

impl CompleteCommand {
    async fn execute(
        &self,
        context: &mut brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::ExecutionResult, brush_core::Error> {
        match self {
            Self::Line { cursor_index, line } => {
                let completions = context
                    .shell
                    .complete(line, cursor_index.unwrap_or(line.len()))
                    .await?;
                for candidate in completions.candidates {
                    writeln!(context.stdout(), "{candidate}")?;
                }
                Ok(ExecutionResult::success())
            }
        }
    }
}
