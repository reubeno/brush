use std::path::Path;

use clap::Parser;

use crate::{builtin, commands, interp::ExecutionParameters};

/// Evalute the provided script in the current shell environment.
#[derive(Debug, Parser)]
pub(crate) struct DotCommand {
    /// Path to the script to evaluate.
    pub script_path: String,

    /// Any arguments to be passed as positional parameters to the script.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub script_args: Vec<String>,
}

#[async_trait::async_trait]
impl builtin::Command for DotCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtin::ExitCode, crate::error::Error> {
        // TODO: Handle trap inheritance.
        let script_args: Vec<_> = self.script_args.iter().map(|a| a.as_str()).collect();

        let result = context
            .shell
            .source(
                Path::new(&self.script_path),
                script_args.as_slice(),
                &ExecutionParameters {
                    open_files: context.open_files.clone(),
                },
            )
            .await?;

        if result.exit_code != 0 {
            return Ok(builtin::ExitCode::Custom(result.exit_code));
        }

        Ok(builtin::ExitCode::Success)
    }
}
