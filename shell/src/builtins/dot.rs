use std::path::Path;

use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Debug, Parser)]
pub(crate) struct DotCommand {
    pub script_path: String,
    pub script_args: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for DotCommand {
    async fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        if !self.script_args.is_empty() {
            log::error!(
                "UNIMPLEMENTED: dot builtin with args: {:?}",
                self.script_args
            );
            return Ok(BuiltinExitCode::Unimplemented);
        }

        //
        // TODO: Handle trap inheritance.
        // TODO: Handle args.
        //

        let script_args: Vec<_> = self.script_args.iter().map(|a| a.as_str()).collect();

        context
            .shell
            .source(Path::new(&self.script_path), script_args.as_slice())
            .await?;

        // TODO: Get exit status from source() above.
        Ok(BuiltinExitCode::Success)
    }
}
