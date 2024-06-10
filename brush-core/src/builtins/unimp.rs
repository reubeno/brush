use crate::{builtin, commands};
use std::io::Write;

use clap::Parser;

/// (UNIMPLEMENTED COMMAND)
#[derive(Parser)]
pub(crate) struct UnimplementedCommand {
    #[clap(allow_hyphen_values = true)]
    pub args: Vec<String>,

    #[clap(skip)]
    declarations: Vec<commands::CommandArg>,
}

#[async_trait::async_trait]
impl builtin::Command for UnimplementedCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtin::ExitCode, crate::error::Error> {
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
        Ok(builtin::ExitCode::Unimplemented)
    }
}

impl builtin::DeclarationCommand for UnimplementedCommand {
    fn set_declarations(&mut self, declarations: Vec<commands::CommandArg>) {
        self.declarations = declarations;
    }
}
