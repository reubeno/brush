use crate::builtin::{BuiltinCommand, BuiltinExitCode};

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
pub(crate) struct UnimplementedCommand {
    #[clap(allow_hyphen_values = true)]
    pub args: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for UnimplementedCommand {
    async fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        log::error!(
            "UNIMPLEMENTED: {}: built-in unimplemented: {} {}",
            context
                .shell
                .shell_name
                .as_ref()
                .map_or("(unknown shell)", |sn| sn),
            context.builtin_name,
            self.args.join(" ")
        );
        Ok(BuiltinExitCode::Unimplemented)
    }
}
