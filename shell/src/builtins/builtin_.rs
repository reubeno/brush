use clap::Parser;
use std::io::Write;

use crate::{
    builtin::{BuiltinCommand, BuiltinExitCode},
    commands,
};

#[derive(Parser)]
pub(crate) struct BuiltiCommand {
    builtin_name: Option<String>,

    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for BuiltiCommand {
    async fn execute(
        &self,
        mut context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        if let Some(builtin_name) = &self.builtin_name {
            if let Some(builtin) = context.shell.builtins.get(builtin_name) {
                context.command_name.clone_from(builtin_name);

                let args: Vec<commands::CommandArg> = std::iter::once(builtin_name.into())
                    .chain(self.args.iter().map(|arg| arg.into()))
                    .collect();

                (builtin.execute_func)(context, args)
                    .await
                    .map(|res: crate::builtin::BuiltinResult| res.exit_code)
            } else {
                writeln!(context.stderr(), "{builtin_name}: command not found")?;
                Ok(BuiltinExitCode::Custom(1))
            }
        } else {
            Ok(BuiltinExitCode::Success)
        }
    }
}
