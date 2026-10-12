use brush_core::{ExecutionResult, sys};
use std::io::Write;

use crate::events;

/// Extension trait for adding brush-specific built-in commands to a shell builder.
pub(crate) trait ShellBuilderBrushBuiltinExt {
    /// Add brush-specific builtins to a shell being built.
    #[must_use]
    fn brush_builtins(self) -> Self;
}

impl<SE: brush_core::extensions::ShellExtensions, S: brush_core::ShellBuilderState>
    ShellBuilderBrushBuiltinExt for brush_core::ShellBuilder<SE, S>
{
    fn brush_builtins(self) -> Self {
        // For compatibility with previous releases, we register the command under both
        // `brushctl` and `brushinfo` names. It will behave identically across the two.
        self.builtin(
            "brushctl",
            brush_core::builtins::builtin::<BrushCtlCommand, _>(),
        )
        .builtin(
            "brushinfo",
            brush_core::builtins::builtin::<BrushCtlCommand, _>(),
        )
    }
}

/// Configure the running brush shell.
#[derive(winnow_args::Args)]
#[arg(
    name = "brushctl",
    disable_help_short,
    disable_version_flag,
    disable_help_subcommand
)]
pub(crate) struct BrushCtlCommand {
    #[arg(subcommand)]
    command_group: CommandGroup,
}

#[derive(winnow_args::Subcommand)]
enum CommandGroup {
    Complete(CompleteGroup),
    Call(CallGroup),
    Events(EventsGroup),
    Process(ProcessGroup),
}

#[derive(winnow_args::Args)]
struct CompleteGroup {
    #[arg(subcommand)]
    command: CompleteCommand,
}

#[derive(winnow_args::Args)]
struct CallGroup {
    #[arg(subcommand)]
    command: CallCommand,
}

#[derive(winnow_args::Args)]
struct EventsGroup {
    #[arg(subcommand)]
    command: EventsCommand,
}

#[derive(winnow_args::Args)]
struct ProcessGroup {
    #[arg(subcommand)]
    command: ProcessCommand,
}

/// Commands for inspecting call state.
#[derive(winnow_args::Subcommand)]
enum CallCommand {
    /// Display the current call stack.
    #[arg(name = "stack")]
    ShowCallStack(ShowCallStack),
}

/// Arguments for `brushctl call stack`.
#[derive(winnow_args::Args)]
struct ShowCallStack {
    /// Whether to show more details.
    #[arg(short = 'd', long = "detailed")]
    detailed: bool,
}

/// Commands for generating completions.
#[derive(winnow_args::Subcommand)]
enum CompleteCommand {
    /// Generate completions for an input line.
    #[arg(name = "line")]
    Line(CompleteLine),
}

/// Arguments for `brushctl complete line`.
#[derive(winnow_args::Args)]
struct CompleteLine {
    /// The 0-indexed cursor position for generation.
    #[arg(long = "cursor", short = 'c')]
    cursor_index: Option<usize>,

    /// The input line to generate completions for.
    #[arg(positional)]
    line: String,
}

/// Commands for configuring tracing events.
#[derive(winnow_args::Subcommand)]
enum EventsCommand {
    /// Display status of enabled events.
    Status,
    /// Enable event.
    Enable(EventName),
    /// Disable event.
    Disable(EventName),
}

/// The event named by `brushctl events enable` and `disable`.
#[derive(winnow_args::Args)]
struct EventName {
    /// Event to enable or disable.
    #[arg(positional)]
    event: events::TraceEvent,
}

/// Commands for inspecting process state.
#[expect(clippy::enum_variant_names)]
#[derive(winnow_args::Subcommand)]
enum ProcessCommand {
    /// Display process ID.
    #[arg(name = "pid")]
    ShowProcessId,
    /// Display process group ID.
    #[arg(name = "pgid")]
    ShowProcessGroupId,
    /// Display foreground process ID.
    #[arg(name = "fgpid")]
    ShowForegroundProcessId,
    /// Display parent process ID.
    #[arg(name = "ppid")]
    ShowParentProcessId,
}

brush_builtin_winnow::winnow_builtin!(BrushCtlCommand);

impl brush_core::builtins::Command for BrushCtlCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        match &self.command_group {
            CommandGroup::Call(CallGroup { command }) => command.execute(&context),
            CommandGroup::Complete(CompleteGroup { command }) => {
                command.execute(&mut context).await
            }
            CommandGroup::Events(EventsGroup { command }) => command.execute(&context),
            CommandGroup::Process(ProcessGroup { command }) => command.execute(&context),
        }
    }
}

impl CallCommand {
    fn execute(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
    ) -> Result<brush_core::ExecutionResult, brush_core::Error> {
        match self {
            Self::ShowCallStack(ShowCallStack { detailed }) => {
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
    async fn execute(
        &self,
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
    ) -> Result<brush_core::ExecutionResult, brush_core::Error> {
        match self {
            Self::Line(CompleteLine { cursor_index, line }) => {
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
    fn execute(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
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
                Self::Enable(EventName { event }) => event_config.enable(*event)?,
                Self::Disable(EventName { event }) => event_config.disable(*event)?,
            }

            Ok(brush_core::ExecutionResult::success())
        } else {
            Err(brush_core::ErrorKind::Unimplemented("event configuration not initialized").into())
        }
    }
}

impl ProcessCommand {
    fn execute(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
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
