use clap::Parser;
use std::io::Write;

use crate::{builtins, commands, error, sys, traps};

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

#[async_trait::async_trait]
impl builtins::Command for TrapCommand {
    async fn execute(
        &self,
        mut context: commands::ExecutionContext<'_>,
    ) -> Result<builtins::ExitCode, crate::error::Error> {
        if self.list_signals {
            Self::display_signals(&context)?;
            Ok(builtins::ExitCode::Success)
        } else if self.print_trap_commands || self.args.is_empty() {
            if !self.args.is_empty() {
                for signal_type in &self.args {
                    let signal_type = parse_signal(signal_type)?;
                    Self::display_handlers_for(&context, signal_type)?;
                }
            } else {
                Self::display_all_handlers(&context)?;
            }
            Ok(builtins::ExitCode::Success)
        } else if self.args.len() == 1 {
            let signal = self.args[0].as_str();
            let signal_type = parse_signal(signal)?;
            Self::remove_all_handlers(&mut context, signal_type);
            Ok(builtins::ExitCode::Success)
        } else {
            let handler = &self.args[0];

            let mut signal_types = vec![];
            for signal in &self.args[1..] {
                signal_types.push(parse_signal(signal)?);
            }

            Self::register_handler(&mut context, signal_types, handler.as_str());
            Ok(builtins::ExitCode::Success)
        }
    }
}

impl TrapCommand {
    fn display_signals(context: &commands::ExecutionContext<'_>) -> Result<(), error::Error> {
        #[cfg(unix)]
        for signal in nix::sys::signal::Signal::iterator() {
            writeln!(context.stdout(), "{}: {signal}", signal as i32)?;
        }

        Ok(())
    }

    fn display_all_handlers(context: &commands::ExecutionContext<'_>) -> Result<(), error::Error> {
        for signal in context.shell.traps.handlers.keys() {
            Self::display_handlers_for(context, *signal)?;
        }
        Ok(())
    }

    fn display_handlers_for(
        context: &commands::ExecutionContext<'_>,
        signal_type: traps::TrapSignal,
    ) -> Result<(), error::Error> {
        if let Some(handler) = context.shell.traps.handlers.get(&signal_type) {
            writeln!(context.stdout(), "trap -- '{handler}' {signal_type}")?;
        }
        Ok(())
    }

    fn remove_all_handlers(
        context: &mut crate::commands::ExecutionContext<'_>,
        signal: traps::TrapSignal,
    ) {
        context.shell.traps.remove_handlers(signal);
    }

    fn register_handler(
        context: &mut crate::commands::ExecutionContext<'_>,
        signals: Vec<traps::TrapSignal>,
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

fn parse_signal(signal: &str) -> Result<traps::TrapSignal, error::Error> {
    if signal.chars().all(|c| c.is_ascii_digit()) {
        let digits = signal
            .parse::<i32>()
            .map_err(|_| error::Error::InvalidSignal)?;

        sys::signal::parse_numeric_signal(digits)
    } else {
        let mut signal_to_parse = signal.to_ascii_uppercase();

        if !signal_to_parse.starts_with("SIG") {
            signal_to_parse.insert_str(0, "SIG");
        }

        match signal_to_parse {
            s if s == "SIGDEBUG" => Ok(traps::TrapSignal::Debug),
            s if s == "SIGERR" => Ok(traps::TrapSignal::Err),
            s if s == "SIGEXIT" => Ok(traps::TrapSignal::Exit),
            s => sys::signal::parse_os_signal_name(s.as_str()),
        }
    }
}
