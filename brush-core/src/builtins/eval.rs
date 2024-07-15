use crate::{builtins, commands, interp::ExecutionParameters};
use clap::Parser;

/// Evalute the given string as script.
#[derive(Parser)]
pub(crate) struct EvalCommand {
    /// The script to evaluate.
    #[clap(allow_hyphen_values = true)]
    pub args: Vec<String>,
}

#[async_trait::async_trait]
impl builtins::Command for EvalCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        if !self.args.is_empty() {
            let args_concatenated = self.args.join(" ");

            tracing::debug!("Applying eval to: {:?}", args_concatenated);

            let exec_result = context
                .shell
                .run_string(
                    args_concatenated,
                    &ExecutionParameters {
                        open_files: context.open_files.clone(),
                    },
                )
                .await?;

            Ok(builtins::ExitCode::Custom(exec_result.exit_code))
        } else {
            Ok(builtins::ExitCode::Success)
        }
    }
}
