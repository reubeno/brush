use clap::Parser;
use std::io::Write;

use brush_core::builtins;

/// Directly invokes a built-in, without going through typical search order.
#[derive(Default, Parser)]
pub(crate) struct BuiltinCommand {
    #[clap(skip)]
    args: Vec<brush_core::CommandArg>,
}

impl builtins::DeclarationCommand for BuiltinCommand {
    fn set_declarations(&mut self, args: Vec<brush_core::CommandArg>) {
        self.args = args;
    }
}

impl builtins::Command for BuiltinCommand {
    async fn execute(
        &self,
        mut context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::builtins::ExitCode, brush_core::Error> {
        if self.args.is_empty() {
            return Ok(builtins::ExitCode::Success);
        }

        let args: Vec<_> = self.args.iter().skip(1).cloned().collect();
        if args.is_empty() {
            return Ok(builtins::ExitCode::Success);
        }

        let builtin_name = args[0].to_string();

        if let Some(builtin) = context.shell.builtins().get(&builtin_name) {
            if !builtin.disabled {
                context.command_name = builtin_name;

                return (builtin.execute_func)(context, args)
                    .await
                    .map(|res: builtins::BuiltinResult| res.exit_code);
            }
        }

        writeln!(context.stderr(), "{builtin_name}: builtin not found")?;
        Ok(builtins::ExitCode::Custom(1))
    }
}
