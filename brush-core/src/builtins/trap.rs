use clap::Parser;
use std::io::Write;

use crate::traps::TrapSignal;
use crate::{builtins, commands, error};

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
    async fn execute(
        &self,
        mut context: commands::ExecutionContext<'_>,
    ) -> Result<builtins::ExitCode, crate::error::Error> {
        if self.list_signals {
            crate::traps::format_signals(context.stdout(), TrapSignal::iterator())
                .map(|()| builtins::ExitCode::Success)
        } else if self.print_trap_commands || self.args.is_empty() {
            if !self.args.is_empty() {
                for signal_type in &self.args {
                    Self::display_handlers_for(&context, signal_type.parse()?)?;
                }
            } else {
                Self::display_all_handlers(&context)?;
            }
            Ok(builtins::ExitCode::Success)
        } else if self.args.len() == 1 {
            let signal = self.args[0].as_str();
            Self::remove_all_handlers(&mut context, signal.parse()?);
            Ok(builtins::ExitCode::Success)
        } else {
            let handler = &self.args[0];

            let mut signal_types = vec![];
            for signal in &self.args[1..] {
                signal_types.push(signal.parse()?);
            }

            Self::register_handler(&mut context, signal_types, handler.as_str());
            Ok(builtins::ExitCode::Success)
        }
    }
}

#[allow(unused_variables)]
impl TrapCommand {
    fn display_all_handlers(context: &commands::ExecutionContext<'_>) -> Result<(), error::Error> {
        for signal in context.shell.traps.handlers.keys() {
            Self::display_handlers_for(context, *signal)?;
        }
        Ok(())
    }

    fn display_handlers_for(
        context: &commands::ExecutionContext<'_>,
        signal_type: TrapSignal,
    ) -> Result<(), error::Error> {
        if let Some(handler) = context.shell.traps.handlers.get(&signal_type) {
            writeln!(context.stdout(), "trap -- '{handler}' {signal_type}")?;
        }
        Ok(())
    }

    fn remove_all_handlers(
        context: &mut crate::commands::ExecutionContext<'_>,
        signal: TrapSignal,
    ) {
        context.shell.traps.remove_handlers(signal);
    }

    fn register_handler(
        context: &mut crate::commands::ExecutionContext<'_>,
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
