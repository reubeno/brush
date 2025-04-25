use clap::{Parser, Subcommand};
use std::io::Write;

use crate::events;

pub(crate) fn register(shell: &mut brush_core::Shell) {
    shell.register_builtin(
        "brushctl",
        brush_core::builtins::builtin::<BrushCtlCommand>(),
    );
}

/// Configure the running brush shell.
#[derive(Parser)]
struct BrushCtlCommand {
    #[clap(subcommand)]
    command_group: CommandGroup,
}

#[derive(Subcommand)]
enum CommandGroup {
    #[clap(subcommand)]
    Events(EventsCommand),
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

impl brush_core::builtins::Command for BrushCtlCommand {
    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::builtins::ExitCode, brush_core::Error> {
        match self.command_group {
            CommandGroup::Events(ref events) => events.execute(&context),
        }
    }
}

impl EventsCommand {
    fn execute(
        &self,
        context: &brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::builtins::ExitCode, brush_core::Error> {
        let event_config = crate::get_event_config();

        let mut event_config = event_config.try_lock().map_err(|_| {
            brush_core::Error::Unimplemented("Failed to acquire lock on event configuration")
        })?;

        if let Some(event_config) = event_config.as_mut() {
            match self {
                EventsCommand::Status => {
                    let enabled_events = event_config.get_enabled_events();
                    for event in enabled_events {
                        writeln!(context.stdout(), "{event}").unwrap(); // Add .unwrap() to handle
                                                                        // any potential write
                                                                        // errors
                    }
                }
                EventsCommand::Enable { event } => event_config.enable(event)?,
                EventsCommand::Disable { event } => event_config.disable(event)?,
            }

            Ok(brush_core::builtins::ExitCode::Success)
        } else {
            Err(brush_core::Error::Unimplemented(
                "event configuration not initialized",
            ))
        }
    }
}
