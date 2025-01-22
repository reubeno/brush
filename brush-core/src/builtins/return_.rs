use clap::Parser;
use std::io::Write;

use crate::{builtins, commands};

/// Return from the current function.
#[derive(Parser)]
pub(crate) struct ReturnCommand {
    /// The exit code to return.
    code: Option<i32>,
}

impl builtins::Command for ReturnCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        let code_8bit: u8;
        #[allow(clippy::cast_sign_loss)]
        if let Some(code_32bit) = &self.code {
            code_8bit = (code_32bit & 0xFF) as u8;
        } else {
            code_8bit = context.shell.last_exit_status;
        }

        if context.shell.in_function() || context.shell.in_sourced_script() {
            Ok(builtins::ExitCode::ReturnFromFunctionOrScript(code_8bit))
        } else {
            writeln!(
                context.shell.stderr(),
                "return: can only be used in a function or sourced script"
            )?;
            Ok(builtins::ExitCode::InvalidUsage)
        }
    }
}
