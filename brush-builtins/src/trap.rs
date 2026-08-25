use std::io::Write;

use brush_core::traps::TrapSignal;
use brush_core::{
    ExecutionResult,
    argmodel::{ArgSpec, PositionalSpec},
    builtins,
};

/// Manage signal traps.
pub(crate) struct TrapCommand {
    list_signals: bool,
    print_trap_commands: bool,
    args: Vec<String>,
}

const ID_LIST_SIGNALS: &str = "list_signals";
const ID_PRINT_TRAP_COMMANDS: &str = "print_trap_commands";
const ID_ARGS: &str = "args";

impl builtins::SpecCommand for TrapCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static builtins::argmodel::CommandSpec {
        static SPEC: builtins::argmodel::CommandSpec = builtins::argmodel::CommandSpec {
            args: &[
                ArgSpec::flag(ID_LIST_SIGNALS, &['l'], &[], "List all signal names."),
                ArgSpec::flag(
                    ID_PRINT_TRAP_COMMANDS,
                    &['p'],
                    &[],
                    "Print registered trap commands.",
                ),
            ],
            positionals: &[PositionalSpec::many(ID_ARGS, "ARGS")],
        };

        &SPEC
    }

    fn from_matches(
        values: &mut builtins::argmodel::ParsedValues,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            list_signals: values.flag(ID_LIST_SIGNALS),
            print_trap_commands: values.flag(ID_PRINT_TRAP_COMMANDS),
            args: values.positional_values(ID_ARGS).to_vec(),
        })
    }

    fn about() -> &'static str {
        "Manage signal traps."
    }

    fn synopsis() -> &'static str {
        "[-lp] [ARGS]..."
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        mut context: brush_core::ExecutionContext<'_, SE>,
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
            // "-" as the first argument indicates that the remaining
            // arguments are signal names and we need to remove the handlers for them.
            for signal in &self.args[1..] {
                Self::remove_all_handlers(&mut context, signal.parse()?);
            }
            Ok(ExecutionResult::success())
        } else {
            let handler = &self.args[0];

            let mut signal_types = Vec::with_capacity(self.args.len() - 1);
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
