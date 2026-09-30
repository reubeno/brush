use std::path::Path;

use brush_core::builtins;

/// Evaluate the provided script in the current shell environment.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct DotCommand {
    /// Path to the script to evaluate.
    #[arg(positional, value_name = "filename", double_dash = "automatic")]
    script_path: String,

    /// Any arguments to be passed as positional parameters to the script.
    #[arg(positional, value_name = "arguments", allow_negative_numbers)]
    script_args: Vec<String>,
}

brush_builtin_winnow::winnow_builtin!(DotCommand);

impl builtins::Command for DotCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
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
