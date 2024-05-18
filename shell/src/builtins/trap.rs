use clap::Parser;
use std::{io::Write, str::FromStr};

use crate::{
    builtin::{BuiltinCommand, BuiltinExitCode},
    error, traps,
};

/// Manage signal traps.
#[derive(Parser)]
pub(crate) struct TrapCommand {
    #[arg(short = 'l')]
    list_signals: bool,

    #[arg(short = 'p')]
    print_trap_commands: bool,

    args: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for TrapCommand {
    async fn execute(
        &self,
        mut context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        if self.list_signals {
            Self::display_signals(&context)?;
            Ok(BuiltinExitCode::Success)
        } else if self.print_trap_commands || self.args.is_empty() {
            if !self.args.is_empty() {
                for signal_type in &self.args {
                    let signal_type = parse_signal(signal_type)?;
                    Self::display_handlers_for(&context, signal_type)?;
                }
            } else {
                Self::display_all_handlers(&context)?;
            }
            Ok(BuiltinExitCode::Success)
        } else if self.args.len() == 1 {
            let signal = self.args[0].as_str();
            let signal_type = parse_signal(signal)?;
            Self::remove_all_handlers(&mut context, signal_type)?;
            Ok(BuiltinExitCode::Success)
        } else {
            let handler = &self.args[0];

            let mut signal_types = vec![];
            for signal in &self.args[1..] {
                signal_types.push(parse_signal(signal)?);
            }

            Self::register_handler(&mut context, signal_types, handler.as_str())?;
            Ok(BuiltinExitCode::Success)
        }
    }
}

impl TrapCommand {
    fn display_signals(
        context: &crate::context::CommandExecutionContext<'_>,
    ) -> Result<(), error::Error> {
        for signal in nix::sys::signal::Signal::iterator() {
            writeln!(context.stdout(), "{}: {signal}", signal as i32)?;
        }

        Ok(())
    }

    fn display_all_handlers(
        context: &crate::context::CommandExecutionContext<'_>,
    ) -> Result<(), error::Error> {
        for signal in context.shell.traps.handlers.keys() {
            Self::display_handlers_for(context, *signal)?;
        }
        Ok(())
    }

    fn display_handlers_for(
        context: &crate::context::CommandExecutionContext<'_>,
        signal_type: traps::TrapSignal,
    ) -> Result<(), error::Error> {
        if let Some(handler) = context.shell.traps.handlers.get(&signal_type) {
            writeln!(context.stdout(), "trap -- '{handler}' {signal_type}")?;
        }
        Ok(())
    }

    #[allow(clippy::unnecessary_wraps)]
    fn remove_all_handlers(
        context: &mut crate::context::CommandExecutionContext<'_>,
        signal: traps::TrapSignal,
    ) -> Result<(), error::Error> {
        context.shell.traps.remove_handlers(signal);
        Ok(())
    }

    #[allow(clippy::unnecessary_wraps)]
    fn register_handler(
        context: &mut crate::context::CommandExecutionContext<'_>,
        signals: Vec<traps::TrapSignal>,
        handler: &str,
    ) -> Result<(), error::Error> {
        for signal in signals {
            context
                .shell
                .traps
                .register_handler(signal, handler.to_owned());
        }

        Ok(())
    }
}

fn parse_signal(signal: &str) -> Result<traps::TrapSignal, error::Error> {
    if signal.chars().all(|c| c.is_ascii_digit()) {
        let digits = signal
            .parse::<i32>()
            .map_err(|_| error::Error::InvalidSignal)?;

        Ok(traps::TrapSignal::Signal(
            nix::sys::signal::Signal::try_from(digits).map_err(|_| error::Error::InvalidSignal)?,
        ))
    } else {
        let mut signal_to_parse = signal.to_ascii_uppercase();

        if !signal_to_parse.starts_with("SIG") {
            signal_to_parse.insert_str(0, "SIG");
        }

        match signal_to_parse {
            s if s == "SIGDEBUG" => Ok(traps::TrapSignal::Debug),
            s if s == "SIGERR" => Ok(traps::TrapSignal::Err),
            s if s == "SIGEXIT" => Ok(traps::TrapSignal::Exit),
            _ => Ok(traps::TrapSignal::Signal(
                nix::sys::signal::Signal::from_str(signal_to_parse.as_str())
                    .map_err(|_| error::Error::InvalidSignal)?,
            )),
        }
    }
}
