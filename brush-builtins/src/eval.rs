use brush_core::{ExecutionResult, builtins};
use clap::Parser;

/// Evaluate the given string as script.
#[derive(Parser)]
pub(crate) struct EvalCommand {
    /// The script to evaluate.
    #[clap(allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for EvalCommand {
    type Error = brush_core::Error;

    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if !self.args.is_empty() {
            let args_concatenated = self.args.join(" ");

            tracing::debug!("Applying eval to: {:?}", args_concatenated);

            let params = context.params.clone();
            let exec_result = context.shell.run_string(args_concatenated, &params).await?;

            Ok(exec_result.exit_code.into())
        } else {
            Ok(ExecutionResult::success())
        }
    }
}
