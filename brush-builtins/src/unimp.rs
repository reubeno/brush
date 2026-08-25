use brush_core::{ExecutionExitCode, builtins, trace_categories};

/// (UNIMPLEMENTED COMMAND)
pub(crate) struct UnimplementedCommand {
    args: Vec<String>,
}

impl builtins::SpecCommand for UnimplementedCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            args: matches.trailing().to_vec(),
        })
    }

    // N.B. Arguments are captured verbatim because unimplemented commands may
    // receive any arguments, including flag-like ones.
    fn new<I>(args: I) -> Result<Self, builtins::BuiltinArgParseError>
    where
        I: IntoIterator<Item = String>,
    {
        let mut args: Vec<String> = args.into_iter().collect();

        // N.B. The first argument is the command name itself.
        if !args.is_empty() {
            args.remove(0);
        }

        Ok(Self { args })
    }

    fn takes_trailing_args() -> bool {
        true
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        tracing::warn!(target: trace_categories::UNIMPLEMENTED,
            "unimplemented built-in: {} {}",
            context.command_name,
            self.args.join(" ")
        );
        Ok(ExecutionExitCode::Unimplemented.into())
    }
}
