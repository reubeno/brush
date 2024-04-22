use crate::builtin::{BuiltinCommand, BuiltinExitCode};
use std::io::Write;

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
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        writeln!(
            context.stderr(),
            "UNIMPLEMENTED: {}: built-in unimplemented: {} {}",
            context
                .shell
                .shell_name
                .as_ref()
                .map_or("(unknown shell)", |sn| sn),
            context.command_name,
            self.args.join(" ")
        )?;
        Ok(BuiltinExitCode::Unimplemented)
    }
}
