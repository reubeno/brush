use clap::Parser;
use std::io::Write;

use brush_core::builtins;

/// Return from the current function.
#[derive(Parser)]
pub(crate) struct ReturnCommand {
    /// The exit code to return.
    code: Option<i32>,
}

impl builtins::Command for ReturnCommand {
    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::builtins::ExitCode, brush_core::Error> {
        #[expect(clippy::cast_sign_loss)]
        let code_8bit = if let Some(code_32bit) = &self.code {
            (code_32bit & 0xFF) as u8
        } else {
            context.shell.last_exit_status
        };

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
