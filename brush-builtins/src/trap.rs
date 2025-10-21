use clap::Parser;
use std::io::Write;

use brush_core::traps::TrapSignal;
use brush_core::{ExecutionResult, builtins};

/// Manage signal traps.
#[derive(Parser)]
pub(crate) struct TrapCommand {
    /// List all signal names.
    #[arg(short = 'l')]
    list_signals: bool,

    /// Print registered trap commands.
    #[arg(short = 'p')]
    print_trap_commands: bool,

    args: Vec<String>,
}

impl builtins::Command for TrapCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        mut context: brush_core::ExecutionContext<'_>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.list_signals {
            brush_core::traps::format_signals(context.stdout(), TrapSignal::iterator())
                .map(|()| ExecutionResult::success())
        } else if self.print_trap_commands || self.args.is_empty() {
            if !self.args.is_empty() {
                for signal_type in &self.args {
                    Self::display_handlers_for(&context, signal_type.parse()?)?;
                }
            } else {
                Self::display_all_handlers(&context)?;
            }
            Ok(ExecutionResult::success())
        } else if self.args.len() == 1 {
            // When only a single argument is given, it is assumed to be a signal name
            // and an indication to remove the handlers for that signal.
            let signal = self.args[0].as_str();
            Self::remove_all_handlers(&mut context, signal.parse()?);
            Ok(ExecutionResult::success())
        } else if self.args[0] == "-" {
            // Alternatively, "-" as the first argument indicates that the next
            // argument is a signal name and we need to remove the handlers for that signal.
            let signal = self.args[1].as_str();
            Self::remove_all_handlers(&mut context, signal.parse()?);
            Ok(ExecutionResult::success())
        } else {
            let handler = &self.args[0];

            let mut signal_types = vec![];
            for signal in &self.args[1..] {
                signal_types.push(signal.parse()?);
            }

            Self::register_handler(&mut context, signal_types, handler.as_str());
            Ok(ExecutionResult::success())
        }
    }
}

impl TrapCommand {
    fn display_all_handlers(
        context: &brush_core::ExecutionContext<'_>,
    ) -> Result<(), brush_core::Error> {
        for (signal, _) in context.shell.traps.iter_handlers() {
            Self::display_handlers_for(context, signal)?;
        }
        Ok(())
    }

    fn display_handlers_for(
        context: &brush_core::ExecutionContext<'_>,
        signal_type: TrapSignal,
    ) -> Result<(), brush_core::Error> {
        if let Some(handler) = context.shell.traps.get_handler(signal_type) {
            writeln!(context.stdout(), "trap -- '{handler}' {signal_type}")?;
        }
        Ok(())
    }

    fn remove_all_handlers(context: &mut brush_core::ExecutionContext<'_>, signal: TrapSignal) {
        context.shell.traps.remove_handlers(signal);
    }

    fn register_handler(
        context: &mut brush_core::ExecutionContext<'_>,
        signals: Vec<TrapSignal>,
        handler: &str,
    ) {
        for signal in signals {
            context
                .shell
                .traps
                .register_handler(signal, handler.to_owned());
        }
    }
}
