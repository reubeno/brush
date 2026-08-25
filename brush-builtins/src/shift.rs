use bpaf::Parser;

use brush_core::{ExecutionExitCode, ExecutionResult, builtins};

/// Shift positional arguments.
pub(crate) struct ShiftCommand {
    n: Option<i32>,
}

impl builtins::Command for ShiftCommand {
    type Error = brush_core::Error;

    fn parser() -> impl bpaf::Parser<Self> {
        let n = bpaf::positional::<i32>("N")
            .help("Number of positions to shift the arguments by (defaults to 1).")
            .optional();
        bpaf::construct!(ShiftCommand { n })
    }

    fn about() -> &'static str {
        "Shift positional arguments."
    }

    fn synopsis() -> &'static str {
        "[N]"
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let n = self.n.unwrap_or(1);

        if n < 0 {
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        #[expect(clippy::cast_sign_loss)]
        let n = n as usize;

        let args = context.shell.current_shell_args_mut();

        if n > args.len() {
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        args.drain(0..n);

        Ok(ExecutionResult::success())
    }
}
