use bpaf::Bpaf;
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
            brush_core::builtins::builtin::<BrushCtlCommand, SE>(),
        )
        .builtin(
            "brushinfo",
            brush_core::builtins::builtin::<BrushCtlCommand, SE>(),
        )
    }
}

/// Configure the running brush shell.
#[derive(Clone, Bpaf, Debug)]
pub(crate) enum CommandGroup {
    /// Generate completions for an input line.
    #[bpaf(command("complete"))]
    Complete {
        /// The 0-indexed cursor position for generation.
        #[bpaf(short('c'), long("cursor"))]
        cursor_index: Option<usize>,
        /// The input line to generate completions for.
        #[bpaf(positional("LINE"))]
        line: String,
    },
    /// Display the current call stack.
    #[bpaf(command("call"))]
    Call {
        #[bpaf(external(show_call_stack), hide)]
        show_call_stack: ShowCallStack,
    },
    /// Configure tracing events.
    #[bpaf(command("events"))]
    Events {
        #[bpaf(external(events_action))]
        events_action: EventsAction,
    },
    /// Inspect process state.
    #[bpaf(command("process"))]
    Process {
        #[bpaf(external(process_info), hide)]
        process_info: ProcessInfo,
    },
}

/// Commands for displaying the current call stack.
#[derive(Clone, Bpaf, Debug)]
pub(crate) struct ShowCallStack {
    /// Whether to show more details.
    #[bpaf(short('d'), long("detailed"))]
    detailed: bool,
}

/// Commands for configuring tracing events.
#[derive(Clone, Bpaf, Debug)]
pub(crate) enum EventsAction {
    /// Display status of enabled events.
    #[bpaf(command("status"))]
    Status,
    /// Enable event.
    #[bpaf(command("enable"))]
    Enable {
        /// Event to enable.
        #[bpaf(positional("EVENT"))]
        event: events::TraceEvent,
    },
    /// Disable event.
    #[bpaf(command("disable"))]
    Disable {
        /// Event to disable.
        #[bpaf(positional("EVENT"))]
        event: events::TraceEvent,
    },
}

/// Commands for inspecting process state.
#[derive(Clone, Bpaf, Debug)]
pub(crate) enum ProcessInfo {
    /// Display process ID.
    #[bpaf(command("pid"))]
    Pid,
    /// Display process group ID.
    #[bpaf(command("pgid"))]
    Pgid,
    /// Display foreground process ID.
    #[bpaf(command("fgpid"))]
    Fgpid,
    /// Display parent process ID.
    #[bpaf(command("ppid"))]
    Ppid,
}

pub(crate) struct BrushCtlCommand {
    command_group: CommandGroup,
}

impl BrushCtlCommand {
    pub(crate) const fn new(group: CommandGroup) -> Self {
        Self {
            command_group: group,
        }
    }
}

impl brush_core::builtins::Command for BrushCtlCommand {
    type Error = brush_core::Error;

    fn parser() -> impl bpaf::Parser<Self> {
        let command_group = command_group();
        bpaf::construct!(BrushCtlCommand { command_group })
    }

    fn about() -> &'static str {
        "Configure the running brush shell."
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        match &self.command_group {
            CommandGroup::Call { show_call_stack } => show_call_stack.execute(&context),
            CommandGroup::Complete { cursor_index, line } => {
                execute_complete_line(&mut context, *cursor_index, line).await
            }
            CommandGroup::Events { events_action } => events_action.execute(&context),
            CommandGroup::Process { process_info } => process_info.execute(&context),
        }
    }
}

impl ShowCallStack {
    fn execute(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
    ) -> Result<brush_core::ExecutionResult, brush_core::Error> {
        let Self { detailed } = self;
        {
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

async fn execute_complete_line(
    context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
    cursor_index: Option<usize>,
    line: &str,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let completions = context
        .shell
        .complete(line, cursor_index.unwrap_or(line.len()))
        .await?;
    for candidate in completions.candidates {
        writeln!(context.stdout(), "{candidate}")?;
    }

    Ok(ExecutionResult::success())
}

impl EventsAction {
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
                Self::Enable { event } => event_config.enable(*event)?,
                Self::Disable { event } => event_config.disable(*event)?,
            }

            Ok(brush_core::ExecutionResult::success())
        } else {
            Err(brush_core::ErrorKind::Unimplemented("event configuration not initialized").into())
        }
    }
}

impl ProcessInfo {
    fn execute(
        &self,
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
    ) -> Result<brush_core::ExecutionResult, brush_core::Error> {
        match self {
            Self::Pid => {
                writeln!(context.stdout(), "{}", std::process::id())?;
                Ok(ExecutionResult::success())
            }
            Self::Pgid => {
                if let Some(pgid) = sys::terminal::get_process_group_id() {
                    writeln!(context.stdout(), "{pgid}")?;
                    Ok(ExecutionResult::success())
                } else {
                    writeln!(context.stderr(), "failed to get process group ID")?;
                    Ok(ExecutionResult::general_error())
                }
            }
            Self::Fgpid => {
                if let Some(pid) = sys::terminal::get_foreground_pid() {
                    writeln!(context.stdout(), "{pid}")?;
                    Ok(ExecutionResult::success())
                } else {
                    writeln!(context.stderr(), "failed to get foreground process ID")?;
                    Ok(ExecutionResult::general_error())
                }
            }
            Self::Ppid => {
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
