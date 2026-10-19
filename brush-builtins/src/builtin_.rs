use clap::Parser;

use brush_core::{ExecutionResult, builtins};

/// Directly invokes a built-in, without going through typical search order.
#[derive(Parser)]
pub(crate) struct BuiltinCommand {
    #[clap(skip)]
    args: Vec<brush_core::CommandArg>,
}

brush_clap_adapter::impl_parse_args!(
    BuiltinCommand,
    parse = |invocation: builtins::Invocation<'_>| Ok(Self {
        args: invocation.into_args()
    }),
    declaration_utility
);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for BuiltinCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let args = self.args;
        if args.is_empty() {
            return Ok(ExecutionResult::success());
        }

        let builtin_name = args[0].to_string();

        if let Some(builtin) = context
            .shell
            .builtins()
            .get(&builtin_name)
            .filter(|builtin| builtin.is_enabled())
            .cloned()
        {
            context.command_name = builtin_name;
            builtin.execute(context, args).await
        } else {
            Err(brush_core::ErrorKind::BuiltinNotFound(builtin_name).into())
        }
    }
}
