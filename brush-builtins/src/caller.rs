use brush_core::writeln_async;
use brush_core::{ExecutionResult, builtins, callstack};

/// Return the context of the current subroutine call.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct CallerCommand {
    /// The number of call frames to go back.
    // Signed as in bash: a number too large for i64 is invalid, not just a missing frame.
    #[arg(positional)]
    expr: Option<i64>,
}

brush_winnow_args_adapter::impl_parse_args!(CallerCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for CallerCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        let stack = context.shell.call_stack();

        let expr = self.expr.unwrap_or(0);

        let frames: Vec<_> = stack
            .iter()
            .filter(|frame| frame.frame_type.is_function() || frame.frame_type.is_script())
            .collect();

        // Look for the last-known location in the parent of frame N.
        let Some(calling_frame) = usize::try_from(expr)
            .ok()
            .and_then(|n| n.checked_add(1))
            .and_then(|n| frames.get(n))
        else {
            return Ok(ExecutionResult::general_error());
        };

        let line = calling_frame.current_line().unwrap_or(1);
        let filename = &calling_frame.source_info.source;

        // When the expr is provided, we display "LINE FUNCTION_NAME FILENAME"
        // When the expr is omitted, we only display "LINE FILENAME"
        if self.expr.is_some() {
            let function_name = match &calling_frame.frame_type {
                callstack::FrameType::Function(func_call) => func_call.name(),
                callstack::FrameType::Script(..) => "source".into(),
                _ => "".into(),
            };

            writeln_async!(context.stdout_async(), "{line} {function_name} {filename}").await?;
        } else {
            writeln_async!(context.stdout_async(), "{line} {filename}").await?;
        }

        Ok(ExecutionResult::success())
    }
}
