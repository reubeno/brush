use std::path::Path;

use clap::Parser;

use crate::{
    builtin::{BuiltinCommand, BuiltinExitCode},
    interp::ExecutionParameters,
};

#[derive(Debug, Parser)]
pub(crate) struct DotCommand {
    pub script_path: String,

    #[arg(trailing_var_arg = true)]
    pub script_args: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for DotCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
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
            return Ok(crate::builtin::BuiltinExitCode::Custom(result.exit_code));
        }

        Ok(BuiltinExitCode::Success)
    }
}
