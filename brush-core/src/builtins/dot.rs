use std::path::Path;

use clap::Parser;

use crate::{builtins, commands};

/// Evaluate the provided script in the current shell environment.
#[derive(Parser)]
pub(crate) struct DotCommand {
    /// Path to the script to evaluate.
    script_path: String,

    /// Any arguments to be passed as positional parameters to the script.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    script_args: Vec<String>,
}

impl builtins::Command for DotCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        // TODO: Handle trap inheritance.
        let params = context.params.clone();
        let result = context
            .shell
            .source_script(
                Path::new(&self.script_path),
                self.script_args.iter(),
                &params,
            )
            .await?;

        if result.exit_code != 0 {
            return Ok(builtins::ExitCode::Custom(result.exit_code));
        }

        Ok(builtins::ExitCode::Success)
    }
}
