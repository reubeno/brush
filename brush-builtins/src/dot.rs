use std::path::Path;

use brush_core::{ExecutionExitCode, builtins};
use std::io::Write;

/// Evaluate the provided script in the current shell environment.
pub(crate) struct DotCommand {
    /// Path to the script to evaluate.
    script_path: String,

    /// Any arguments to be passed as positional parameters to the script.
    script_args: Vec<String>,
}

impl builtins::SpecCommand for DotCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        let mut trailing = matches.trailing().iter();
        let script_path = trailing.next().cloned().unwrap_or_default();

        Ok(Self {
            script_path,
            script_args: trailing.cloned().collect(),
        })
    }

    fn about() -> &'static str {
        "Evaluate the provided script in the current shell environment."
    }

    fn synopsis() -> &'static str {
        "SCRIPT_PATH [ARGS]..."
    }

    fn takes_trailing_args() -> bool {
        true
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if self.script_path.is_empty() {
            writeln!(
                context.stderr(),
                "{}: filename argument required",
                context.command_name
            )?;
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        // TODO(dot): Handle trap inheritance.
        context
            .shell
            .source_script(
                Path::new(&self.script_path),
                self.script_args.iter(),
                &context.params,
            )
            .await
    }
}
