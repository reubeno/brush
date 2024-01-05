use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser, Debug)]
pub(crate) struct TrapCommand {
    #[arg(short = 'l')]
    list_signals: bool,

    #[arg(short = 'p')]
    print_trap_commands: bool,

    command: Option<String>,
    signals: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for TrapCommand {
    async fn execute(
        &self,
        _context: &mut crate::builtin::BuiltinExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode> {
        if self.list_signals {
            log::error!("UNIMPLEMENTED: trap -l");
            return Ok(BuiltinExitCode::Unimplemented);
        }

        if self.print_trap_commands {
            log::error!("UNIMPLEMENTED: trap -p");
            return Ok(BuiltinExitCode::Unimplemented);
        }

        // TODO: handle case where trap_command is a signal itself
        if let Some(trap_command) = &self.command {
            if self.signals.is_empty() {
                log::error!("UNIMPLEMENTED: trap builtin called with command but no signals");
                return Ok(BuiltinExitCode::Unimplemented);
            }

            for signal in &self.signals {
                match signal.as_str() {
                    "DEBUG" => (),
                    _ => {
                        log::error!("UNIMPLEMENTED: trap builtin called for signal {signal} (command: '{trap_command}')");
                    }
                }
            }
        } else {
            log::error!("UNIMPLEMENTED: trap builtin called without command");
            return Ok(BuiltinExitCode::Unimplemented);
        }

        Ok(BuiltinExitCode::Success)
    }
}
