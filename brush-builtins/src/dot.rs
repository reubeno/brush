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

impl builtins::Command for DotCommand {
    type Error = brush_core::Error;

    fn parser() -> impl bpaf::Parser<Self> {
        // N.B. Only the leading options are parsed here; all remaining tokens
        // are captured verbatim via `takes_trailing_args`.
        let script_path = bpaf::pure(String::new());
        let script_args = bpaf::pure(Vec::new());

        bpaf::construct!(DotCommand {
            script_path,
            script_args,
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

    fn set_trailing_args(&mut self, args: Vec<String>) {
        let mut iter = args.into_iter();
        if let Some(script_path) = iter.next() {
            self.script_path = script_path;
        }
        self.script_args = iter.collect();
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
