use crate::{
    builtin::{BuiltinCommand, BuiltinExitCode},
    interp::ExecutionParameters,
};
use clap::Parser;

/// Evalute the given string as script.
#[derive(Parser)]
pub(crate) struct EvalCommand {
    #[clap(allow_hyphen_values = true)]
    pub args: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for EvalCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        if !self.args.is_empty() {
            let args_concatenated = self.args.join(" ");

            tracing::debug!("Applying eval to: {:?}", args_concatenated);

            let exec_result = context
                .shell
                .run_string(
                    args_concatenated.as_str(),
                    &ExecutionParameters {
                        open_files: context.open_files.clone(),
                    },
                )
                .await?;

            Ok(BuiltinExitCode::Custom(exec_result.exit_code))
        } else {
            Ok(BuiltinExitCode::Success)
        }
    }
}
