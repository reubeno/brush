//! The `trap` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(TrapCommand);

use brush_core::ExecutionResult;
use brush_core::traps::TrapSignal;
use std::io::Write;

impl TrapCommand {
    fn display_all_handlers(
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
    ) -> Result<(), brush_core::Error> {
        for (signal, _) in context.shell.traps().iter_handlers() {
            Self::display_handlers_for(context, signal)?;
        }
        Ok(())
    }

    fn display_handlers_for(
        context: &brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        signal_type: TrapSignal,
    ) -> Result<(), brush_core::Error> {
        if let Some(handler) = context.shell.traps().get_handler(signal_type) {
            writeln!(
                context.stdout(),
                "trap -- '{}' {signal_type}",
                handler.command
            )?;
        }
        Ok(())
    }

    fn remove_all_handlers(
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        signal: TrapSignal,
    ) {
        context.shell.traps_mut().remove_handlers(signal);
    }

    fn register_handler<I>(
        context: &mut brush_core::ExecutionContext<'_, impl brush_core::ShellExtensions>,
        signals: I,
        handler: &str,
    ) where
        I: IntoIterator<Item = TrapSignal>,
    {
        // Our new source context is relative to the current position.
        // TODO(source-info): Provide the location of the specific token that makes up
        // `self.args[0]`.
        let source_info = context.shell.call_stack().current_pos_as_source_info();

        for signal in signals {
            context.shell.traps_mut().register_handler(
                signal,
                handler.to_owned(),
                source_info.clone(),
            );
        }
    }
}

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &TrapCommand,
    mut context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    if command.list_signals {
        brush_core::traps::format_signals(context.stdout(), TrapSignal::iterator())
            .map(|()| ExecutionResult::success())
    } else if command.print_trap_commands || command.args.is_empty() {
        if !command.args.is_empty() {
            for signal_type in &command.args {
                TrapCommand::display_handlers_for(&context, signal_type.parse()?)?;
            }
        } else {
            TrapCommand::display_all_handlers(&context)?;
        }
        Ok(ExecutionResult::success())
    } else if command.args.len() == 1 {
        // When only a single argument is given, it is assumed to be a signal name
        // and an indication to remove the handlers for that signal.
        let signal = command.args[0].as_str();
        TrapCommand::remove_all_handlers(&mut context, signal.parse()?);
        Ok(ExecutionResult::success())
    } else if command.args[0] == "-" {
        // "-" as the first argument indicates that the remaining
        // arguments are signal names and we need to remove the handlers for them.
        for signal in &command.args[1..] {
            TrapCommand::remove_all_handlers(&mut context, signal.parse()?);
        }
        Ok(ExecutionResult::success())
    } else {
        let handler = &command.args[0];

        let mut signal_types = Vec::with_capacity(command.args.len() - 1);
        for signal in &command.args[1..] {
            signal_types.push(signal.parse()?);
        }

        TrapCommand::register_handler(&mut context, signal_types, handler.as_str());
        Ok(ExecutionResult::success())
    }
}
