use brush_core::{ExecutionResult, builtins, callstack};
use std::io::Write;

/// Return the context of the current subroutine call.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct CallerCommand {
    /// The number of call frames to go back.
    #[arg(positional)]
    expr: Option<usize>,
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

        let Some(calling_frame) = frames.get(expr + 1) else {
            return Ok(ExecutionResult::general_error());
        };

        let line = calling_frame.current_line().unwrap_or(1);
        let filename = &calling_frame.source_info.source;

        let mut output = Vec::new();

        if self.expr.is_some() {
            let function_name = match &calling_frame.frame_type {
                callstack::FrameType::Function(func_call) => func_call.name(),
                callstack::FrameType::Script(..) => "source".into(),
                _ => "".into(),
            };

            writeln!(output, "{line} {function_name} {filename}")?;
        } else {
            writeln!(output, "{line} {filename}")?;
        }

        if let Some(mut stdout) = context.stdout_async() {
            stdout.write_all(&output).await?;
            stdout.flush().await?;
        } else {
            context.stdout().write_all(&output)?;
            context.stdout().flush()?;
        }

        Ok(ExecutionResult::success())
    }
}
