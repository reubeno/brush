use clap::Parser;

use brush_core::builtins::UsageError;
use brush_core::{ExecutionControlFlow, ExecutionResult, builtins};

/// Return from the current function.
#[derive(Parser)]
pub(crate) struct ReturnCommand {
    /// The exit code to return.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    code: Vec<String>,
}

brush_clap_adapter::impl_parse_args!(ReturnCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for ReturnCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let code_8bit = match self.code.as_slice() {
            [] => context.shell.last_exit_status(),
            #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            [operand] => crate::operand::number(operand)? as u8,
            _ => return crate::operand::too_many("return", context.stderr()),
        };

        if context.shell.in_function() || context.shell.in_sourced_script() {
            let mut result = ExecutionResult::new(code_8bit);
            result.next_control_flow = ExecutionControlFlow::ReturnFromFunctionOrScript;

            Ok(result)
        } else {
            Err(UsageError::new("can only `return' from a function or sourced script").into())
        }
    }
}
