use brush_core::{ExecutionResult, argmodel::CommandSpec, builtins};

/// Evaluate the given string as script.
pub(crate) struct EvalCommand {
    /// The script to evaluate.
    args: Vec<String>,
}

impl builtins::SpecCommand for EvalCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static CommandSpec {
        &CommandSpec::EMPTY
    }

    fn from_matches(
        values: &mut builtins::argmodel::ParsedValues,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            args: values.trailing().to_vec(),
        })
    }

    fn about() -> &'static str {
        "Evaluate the given string as script."
    }

    fn synopsis() -> &'static str {
        "[COMMAND]..."
    }

    fn takes_trailing_args() -> bool {
        true
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        // N.B. A leading `--` ends eval's (empty) option section and is not
        // part of the script to evaluate.
        let script = self
            .args
            .iter()
            .skip(usize::from(
                self.args.first().map(String::as_str) == Some("--"),
            ))
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");

        if !script.is_empty() {
            tracing::debug!("Applying eval to: {:?}", script);

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
                .run_string(script, &source_info, &context.params)
                .await
        } else {
            Ok(ExecutionResult::success())
        }
    }
}
