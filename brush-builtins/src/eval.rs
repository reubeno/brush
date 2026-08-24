use brush_core::{ExecutionResult, builtins};

/// Evaluate the given string as script.
#[derive(usage::Cli)]
#[usage(bin = "eval", unknown_flags = "value", args_override_self = false)]
pub(crate) struct EvalCommand {
    /// The script to evaluate.
    // TODO(usage-migration): usage rejects `allow_hyphen_values` on a positional; it is
    // normalized into the `trailing_var_arg` boundary (`double_dash = "automatic"`).
    #[usage(trailing_var_arg, allow_hyphen_values)]
    args: Vec<String>,
}

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

brush_core::impl_usage_parse!(EvalCommand);
