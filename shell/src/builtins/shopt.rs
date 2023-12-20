use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser, Debug)]
pub(crate) struct ShoptCommand {
    #[arg(short = 'o')]
    set_o_names_only: bool,

    #[arg(short = 'p')]
    print: bool,

    #[arg(short = 'q')]
    quiet: bool,

    #[arg(short = 's')]
    set: bool,

    #[arg(short = 'u')]
    unset: bool,

    options: Vec<String>,
}

impl BuiltinCommand for ShoptCommand {
    fn execute(
        &self,
        _context: &mut crate::builtin::BuiltinExecutionContext,
    ) -> Result<crate::builtin::BuiltinExitCode> {
        if self.print {
            log::error!("UNIMPLEMENTED: shopt option: print; options: {:?}", self);
            return Ok(BuiltinExitCode::Unimplemented);
        }

        if self.quiet {
            log::debug!("UNIMPLEMENTED: shopt option: quiet; options: {:?}", self);
        }

        if self.options.is_empty() {
            log::error!("UNIMPLEMENTED: shopt: no options provided");
            return Ok(BuiltinExitCode::Unimplemented);
        }

        if self.set_o_names_only {
            for option in &self.options {
                match option.as_str() {
                    "posix" => {
                        // TODO: implement posix flag
                    }
                    _ => {
                        log::error!("UNIMPLEMENTED: shopt: option '{}'", option);
                        return Ok(BuiltinExitCode::Unimplemented);
                    }
                }
            }
        } else {
            for option in &self.options {
                match option.as_str() {
                    "checkwinsize" => {
                        // TODO: implement updating LINES/COLUMNS
                    }
                    "histappend" => {
                        // TODO: implement history policy
                    }
                    _ => {
                        log::error!("UNIMPLEMENTED: shopt: option '{}'", option);
                        return Ok(BuiltinExitCode::Unimplemented);
                    }
                }
            }
        }

        Ok(BuiltinExitCode::Success)
    }
}
