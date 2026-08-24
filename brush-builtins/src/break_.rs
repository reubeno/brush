use bpaf::Parser;

use brush_core::{ExecutionControlFlow, ExecutionExitCode, ExecutionResult, builtins};

/// Breaks out of a control-flow loop.
pub(crate) struct BreakCommand {
    which_loop: i8,
}

impl builtins::Command for BreakCommand {
    type Error = brush_core::Error;

    fn parser() -> impl bpaf::Parser<Self> {
        let which_loop = bpaf::positional::<i8>("WHICH_LOOP")
            .help("If specified, indicates which nested loop to break out of.")
            .fallback(1);
        bpaf::construct!(BreakCommand { which_loop })
    }

    fn about() -> &'static str {
        "Breaks out of a control-flow loop."
    }

    fn synopsis() -> &'static str {
        "[N]"
    }

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
