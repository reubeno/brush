use std::path::Path;

use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Debug, Parser)]
pub(crate) struct DotCommand {
    pub script_path: String,
    pub script_args: Vec<String>,
}

impl BuiltinCommand for DotCommand {
    fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext,
    ) -> Result<crate::builtin::BuiltinExitCode> {
        if self.script_args.len() > 0 {
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
            .source(Path::new(&self.script_path), script_args.as_slice())?;

        // TODO: Get exit status from source() above.
        Ok(BuiltinExitCode::Success)
    }
}
