use std::io::Write;

use brush_core::{ExecutionControlFlow, ExecutionResult, builtins};

/// Continue to the next iteration of a control-flow loop.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct ContinueCommand {
    /// If specified, indicates which nested loop to continue to the next iteration of.
    #[arg(positional, default = "1", allow_negative_numbers)]
    which_loop: i8,
}

brush_winnow_args_adapter::impl_parse_args!(ContinueCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for ContinueCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        // A count that isn't positive is reported.
        if self.which_loop <= 0 {
            writeln!(
                context.stderr(),
                "{}: {}: loop count out of range",
                context.command_name,
                self.which_loop
            )?;
            return Ok(ExecutionResult::general_error());
        }

        let mut result = ExecutionResult::success();

        result.next_control_flow = ExecutionControlFlow::ContinueLoop {
            #[expect(clippy::cast_sign_loss)]
            levels: (self.which_loop - 1) as usize,
        };

        Ok(result)
    }
}
