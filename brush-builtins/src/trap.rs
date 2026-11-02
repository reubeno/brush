use std::io::Write;

use brush_core::traps::TrapSignal;
use brush_core::{ExecutionResult, builtins};

/// Manage signal traps.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct TrapCommand {
    /// List all signal names.
    #[arg(short = 'l')]
    list_signals: bool,

    /// Print registered trap commands.
    #[arg(short = 'p')]
    print_trap_commands: bool,

    #[arg(positional, double_dash = "automatic")]
    args: Vec<String>,
}

brush_winnow_args_adapter::impl_parse_args!(TrapCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for TrapCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
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
        } else {
            // A single argument is a signal whose handlers are removed, as are
            // the signals after a leading "-"; otherwise the first argument is
            // the handler for the signals after it.
            let (handler, signals) = match self.args.as_slice() {
                [first, rest @ ..] if !rest.is_empty() => {
                    (Some(first.as_str()).filter(|h| *h != "-"), rest)
                }
                all => (None, all),
            };

            // An invalid signal is reported and fails the command, but the
            // valid ones around it are still updated.
            let mut result = ExecutionResult::success();
            for signal in signals {
                match signal.parse() {
                    Ok(signal) => match handler {
                        Some(handler) => {
                            Self::register_handler(&mut context, [signal], handler);
                        }
                        None => Self::remove_all_handlers(&mut context, signal),
                    },
                    Err(e) => {
                        writeln!(context.stderr(), "{}: {e}", context.command_name)?;
                        result = ExecutionResult::general_error();
                    }
                }
            }
            Ok(result)
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
