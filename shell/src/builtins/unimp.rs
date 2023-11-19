use crate::builtin::{BuiltinCommand, BuiltinExitCode};

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
pub(crate) struct UnimplementedCommand {
    pub args: Vec<String>,
}

impl BuiltinCommand for UnimplementedCommand {
    fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext,
    ) -> Result<crate::builtin::BuiltinExitCode> {
        log::error!(
            "built-in unimplemented: {} {}",
            context.builtin_name,
            self.args.join(" ")
        );
        Ok(BuiltinExitCode::Unimplemented)
    }
}
