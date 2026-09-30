use brush_core::{ExecutionResult, builtins};
use usage::Cli;

/// Evaluate the given string as script.
#[derive(Cli)]
#[usage(bin = "eval", unknown_flags = "value", args_override_self = false)]
pub(crate) struct EvalCommand {
    /// The script to evaluate.
    #[usage(trailing_var_arg, allow_hyphen_values)]
    args: Vec<String>,
}

brush_builtin_usage::usage_builtin!(EvalCommand);

impl builtins::Command for EvalCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if !self.args.is_empty() {
            let args_concatenated = self.args.join(" ");

            tracing::debug!("Applying eval to: {:?}", args_concatenated);

            // Our new source context is relative to the current position because we are only
            // providing the raw string being eval'd.
            // TODO(source-info): Provide the location of the specific tokens that make up
            // `self.args`.
            let source_info = context.shell.call_stack().current_pos_as_source_info();

            // Return the direct result of running the string; we intentionally
            // pass through the result and honor its requested control flow. eval
            // executes in the current environment, so all control flow (return,
            // exit, break, continue) should propagate.
            context
                .shell
                .run_string(args_concatenated, &source_info, &context.params)
                .await
        } else {
            Ok(ExecutionResult::success())
        }
    }
}
