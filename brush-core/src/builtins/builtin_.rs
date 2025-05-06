use clap::Parser;
use std::io::Write;

use crate::{builtins, commands};

/// Directly invokes a built-in, without going through typical search order.
#[derive(Default, Parser)]
pub(crate) struct BuiltinCommand {
    #[clap(skip)]
    args: Vec<commands::CommandArg>,
}

impl builtins::DeclarationCommand for BuiltinCommand {
    fn set_declarations(&mut self, args: Vec<commands::CommandArg>) {
        self.args = args;
    }
}

impl builtins::Command for BuiltinCommand {
    async fn execute(
        &self,
        mut context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        if self.args.is_empty() {
            return Ok(builtins::ExitCode::Success);
        }

        let args: Vec<_> = self.args.iter().skip(1).cloned().collect();
        if args.is_empty() {
            return Ok(builtins::ExitCode::Success);
        }

        let builtin_name = args[0].to_string();

        if let Some(builtin) = context.shell.builtins.get(&builtin_name) {
            context.command_name = builtin_name;

            (builtin.execute_func)(context, args)
                .await
                .map(|res: builtins::BuiltinResult| res.exit_code)
        } else {
            writeln!(context.stderr(), "{builtin_name}: command not found")?;
            Ok(builtins::ExitCode::Custom(1))
        }
    }
}
