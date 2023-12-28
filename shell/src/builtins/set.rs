use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExecutionContext, BuiltinExitCode};

#[derive(Parser, Debug)]
pub(crate) struct SetCommand {
    #[arg(short = 'x')]
    print_commands_and_arguments: bool,

    #[clap(allow_hyphen_values = true)]
    pub unhandled_args: Vec<String>,
}

impl BuiltinCommand for SetCommand {
    fn execute(&self, context: &mut BuiltinExecutionContext) -> Result<BuiltinExitCode> {
        if self.print_commands_and_arguments {
            context.shell.options.print_commands_and_arguments = true;
        } else {
            log::error!("UNIMPLEMENTED: set builtin");
            return Ok(BuiltinExitCode::Unimplemented);
        }

        if !self.unhandled_args.is_empty() {
            log::error!("UNIMPLEMENTED: set builtin");
            return Ok(BuiltinExitCode::Unimplemented);
        }

        Ok(BuiltinExitCode::Success)
    }
}
