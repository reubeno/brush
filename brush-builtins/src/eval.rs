use brush_core::builtins;
use clap::Parser;

/// Evaluate the given string as script.
#[derive(Parser)]
pub(crate) struct EvalCommand {
    /// The script to evaluate.
    #[clap(allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for EvalCommand {
    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::builtins::ExitCode, brush_core::Error> {
        if !self.args.is_empty() {
            let args_concatenated = self.args.join(" ");

            tracing::debug!("Applying eval to: {:?}", args_concatenated);

            let params = context.params.clone();
            let exec_result = context.shell.run_string(args_concatenated, &params).await?;

            Ok(builtins::ExitCode::Custom(exec_result.exit_code))
        } else {
            Ok(builtins::ExitCode::Success)
        }
    }
}
