use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser, Debug)]
pub(crate) struct ExitCommand {
    code: Option<i32>,
}

impl BuiltinCommand for ExitCommand {
    fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext,
    ) -> Result<crate::builtin::BuiltinExitCode> {
        let code_8bit: u8;
        if let Some(code_32bit) = &self.code {
            code_8bit = (code_32bit & 0xFF) as u8;
        } else {
            code_8bit = context.shell.last_exit_status;
        }

        Ok(BuiltinExitCode::ExitShell(code_8bit))
    }
}
