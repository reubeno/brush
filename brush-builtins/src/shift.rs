use clap::Parser;
use std::io::Write;

use brush_core::{ExecutionExitCode, ExecutionResult, builtins};

/// Shift positional arguments.
#[derive(Parser)]
pub(crate) struct ShiftCommand {
    /// Number of positions to shift the arguments by (defaults to 1).
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    n: Vec<String>,
}

brush_clap_adapter::impl_parse_args!(ShiftCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for ShiftCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let n = match self.n.as_slice() {
            [] => 1,
            [operand] => crate::operand::number(operand)?,
            _ => return crate::operand::too_many("shift", context.stderr()),
        };

        if n < 0 {
            writeln!(context.stderr(), "shift: {n}: shift count out of range")?;
            return Ok(ExecutionExitCode::GeneralError.into());
        }

        let args = context.shell.current_shell_args_mut();

        let Some(n) = usize::try_from(n).ok().filter(|n| *n <= args.len()) else {
            return Ok(ExecutionExitCode::GeneralError.into());
        };

        args.drain(0..n);

        Ok(ExecutionResult::success())
    }
}
