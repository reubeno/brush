use clap::Parser;
use std::io::Write;

use crate::{builtins, commands};

/// Directly invokes a built-in, without going through typical search order.
#[derive(Parser)]
pub(crate) struct BuiltinCommand {
    /// Name of built-in to invoke.
    builtin_name: Option<String>,

    /// Arguments for the built-in.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}


impl builtins::Command for BuiltinCommand {
    async fn execute(
        &self,
        mut context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        if let Some(builtin_name) = &self.builtin_name {
            if let Some(builtin) = context.shell.builtins.get(builtin_name) {
                context.command_name.clone_from(builtin_name);

                let args: Vec<commands::CommandArg> = std::iter::once(builtin_name.into())
                    .chain(self.args.iter().map(|arg| arg.into()))
                    .collect();

                (builtin.execute_func)(context, args)
                    .await
                    .map(|res: builtins::BuiltinResult| res.exit_code)
            } else {
                writeln!(context.stderr(), "{builtin_name}: command not found")?;
                Ok(builtins::ExitCode::Custom(1))
            }
        } else {
            Ok(builtins::ExitCode::Success)
        }
    }
}
