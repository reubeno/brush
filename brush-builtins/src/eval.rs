use brush_core::{ExecutionResult, builtins};

/// Evaluate the given string as script.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct EvalCommand {
    /// The script to evaluate.
    #[arg(positional, double_dash = "automatic")]
    args: Vec<String>,
}

brush_winnow_args_adapter::impl_parse_args!(EvalCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for EvalCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if !self.args.is_empty() {
            let args_concatenated = self.args.join(" ");

            tracing::debug!("Applying eval to: {:?}", args_concatenated);

            context.eval(args_concatenated).await
        } else {
            Ok(ExecutionResult::success())
        }
    }
}
