//! The `eval` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(EvalCommand);

use brush_core::ExecutionResult;

async fn execute<SE: brush_core::ShellExtensions>(
    command: &EvalCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    if !command.args.is_empty() {
        let args_concatenated = command.args.join(" ");

        tracing::debug!("Applying eval to: {:?}", args_concatenated);

        // Our new source context is relative to the current position because we are only
        // providing the raw string being eval'd.
        // TODO(source-info): Provide the location of the specific tokens that make up
        // `command.args`.
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
