use clap::Parser;
use std::io::Write;

use brush_core::{ExecutionControlFlow, ExecutionExitCode, ExecutionResult, builtins};

/// Continue to the next iteration of a control-flow loop.
#[derive(Parser)]
pub(crate) struct ContinueCommand {
    /// If specified, indicates which nested loop to continue to the next iteration of.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    which_loop: Vec<String>,
}

brush_clap_adapter::impl_parse_args!(ContinueCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for ContinueCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let which_loop = match self.which_loop.as_slice() {
            [] => 1,
            [operand] => crate::operand::number(operand)?,
            _ => return crate::operand::too_many("continue", context.stderr()),
        };

        // If specified, which_loop needs to be positive.
        if which_loop <= 0 {
            writeln!(
                context.stderr(),
                "continue: {which_loop}: loop count out of range"
            )?;
            return Ok(ExecutionExitCode::GeneralError.into());
        }

        let mut result = ExecutionResult::success();

        result.next_control_flow = ExecutionControlFlow::ContinueLoop {
            levels: usize::try_from(which_loop - 1).unwrap_or(usize::MAX),
        };

        Ok(result)
    }
}
