use brush_core::{ExecutionResult, sys};
use clap::{Parser, Subcommand};
use std::io::Write;

use crate::events;

/// Extension trait for adding brush-specific built-in commands to a shell builder.
pub(crate) trait ShellBuilderBrushBuiltinExt {
    /// Add brush-specific builtins to a shell being built.
    #[must_use]
    fn brush_builtins(self) -> Self;
}

impl<S: brush_core::ShellBuilderState> ShellBuilderBrushBuiltinExt for brush_core::ShellBuilder<S> {
    fn brush_builtins(self) -> Self {
        // For compatibility with previous releases, we register the command under both
        // `brushctl` and `brushinfo` names. It will behave identically across the two.
        self.builtin(
            "brushctl",
            brush_core::builtins::builtin::<BrushCtlCommand, brush_core::Shell>(),
        )
        .builtin(
            "brushinfo",
            brush_core::builtins::builtin::<BrushCtlCommand, brush_core::Shell>(),
        )
    }
}

/// Configure the running brush shell.
#[derive(Parser)]
pub(crate) struct BrushCtlCommand {
    #[clap(subcommand)]
    command_group: CommandGroup,
}

#[derive(Subcommand)]
enum CommandGroup {
    #[clap(subcommand)]
    Complete(CompleteCommand),
    #[clap(subcommand)]
    Call(CallCommand),
    #[clap(subcommand)]
    Events(EventsCommand),
    #[clap(subcommand)]
    Process(ProcessCommand),
}

/// Commands for inspecting call state.
#[derive(Subcommand)]
enum CallCommand {
    /// Display the current call stack.
    #[clap(name = "stack")]
    ShowCallStack {
        /// Whether to show more details.
        #[clap(short = 'd', long = "detailed")]
        detailed: bool,
    },
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

/// Commands for configuring tracing events.
#[derive(Subcommand)]
enum EventsCommand {
    /// Display status of enabled events.
    Status,

    /// Enable event.
    Enable {
        /// Event to enable.
        event: events::TraceEvent,
    },

    /// Disable event.
    Disable {
        /// Event to disable.
        event: events::TraceEvent,
    },
}

/// Commands for inspecting process state.
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

impl brush_core::builtins::Command for BrushCtlCommand {
    type Error = brush_core::Error;

    async fn execute<S: brush_core::ShellRuntime>(
        &self,
        mut context: brush_core::ExecutionContext<'_, S>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        match &self.command_group {
            CommandGroup::Call(call) => call.execute(&context),
            CommandGroup::Complete(complete) => complete.execute(&mut context).await,
            CommandGroup::Events(events) => events.execute(&context),
            CommandGroup::Process(process) => process.execute(&context),
        }
    }
}

impl CallCommand {
    fn execute<S: brush_core::ShellRuntime>(
        &self,
        context: &brush_core::ExecutionContext<'_, S>,
    ) -> Result<brush_core::ExecutionResult, brush_core::Error> {
        match self {
            Self::ShowCallStack { detailed } => {
                let stack = context.shell.call_stack();
                let format_options = brush_core::callstack::FormatOptions {
                    show_args: *detailed,
                    show_entry_points: *detailed,
                };

                write!(context.stdout(), "{}", stack.format(&format_options))?;

                Ok(ExecutionResult::success())
            }
        }
    }
}

impl CompleteCommand {
    async fn execute<S: brush_core::ShellRuntime>(
        &self,
        context: &mut brush_core::ExecutionContext<'_, S>,
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

impl EventsCommand {
    fn execute<S: brush_core::ShellRuntime>(
        &self,
        context: &brush_core::ExecutionContext<'_, S>,
    ) -> Result<brush_core::ExecutionResult, brush_core::Error> {
        let event_config = crate::entry::get_event_config();

        let mut event_config = event_config.try_lock().map_err(|_| {
            brush_core::Error::from(brush_core::ErrorKind::Unimplemented(
                "Failed to acquire lock on event configuration",
            ))
        })?;

        if let Some(event_config) = event_config.as_mut() {
            match self {
                Self::Status => {
                    let enabled_events = event_config.get_enabled_events();
                    for event in enabled_events {
                        writeln!(context.stdout(), "{event}")?;
                    }
                }
                Self::Enable { event } => event_config.enable(*event)?,
                Self::Disable { event } => event_config.disable(*event)?,
            }

            Ok(brush_core::ExecutionResult::success())
        } else {
            Err(brush_core::ErrorKind::Unimplemented("event configuration not initialized").into())
        }
    }
}

impl ProcessCommand {
    fn execute<S: brush_core::ShellRuntime>(
        &self,
        context: &brush_core::ExecutionContext<'_, S>,
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
