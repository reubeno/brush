use brush_core::{ExecutionControlFlow, ExecutionExitCode, ExecutionResult, builtins};

/// Breaks out of a control-flow loop.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct BreakCommand {
    /// If specified, indicates which nested loop to break out of.
    #[arg(positional, default = "1", allow_negative_numbers)]
    which_loop: i8,
}

brush_builtin_winnow::winnow_builtin!(BreakCommand);

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
