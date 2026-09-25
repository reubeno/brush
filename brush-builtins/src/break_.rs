use usage::Cli;

use brush_core::{ExecutionControlFlow, ExecutionExitCode, ExecutionResult, builtins};

/// Breaks out of a control-flow loop.
#[derive(Cli)]
#[usage(bin = "break", unknown_flags = "error", args_override_self = false)]
pub(crate) struct BreakCommand {
    /// If specified, indicates which nested loop to break out of.
    #[usage(default = "1")]
    which_loop: i8,
}

brush_builtin_usage::usage_builtin!(BreakCommand);

impl builtins::Command for BreakCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        _context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        // If specified, which_loop needs to be positive.
        if self.which_loop <= 0 {
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        let mut result = ExecutionResult::success();

        result.next_control_flow = ExecutionControlFlow::BreakLoop {
            #[expect(clippy::cast_sign_loss)]
            levels: (self.which_loop - 1) as usize,
        };

        Ok(result)
    }
}
