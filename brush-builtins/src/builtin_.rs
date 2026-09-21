use brush_core::{ExecutionResult, builtins};

/// Directly invokes a built-in, without going through typical search order.
pub(crate) struct BuiltinCommand {
    args: Vec<brush_core::CommandArg>,
}

brush_builtin_utils::verbatim_builtin!(
    BuiltinCommand,
    args = args,
    synopsis = "builtin [shell-builtin [arg ...]]",
    description = "Execute shell builtins",
    help = "Execute SHELL-BUILTIN with arguments ARGs without performing command lookup.\n",
);

impl builtins::Command for BuiltinCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let Some((name, args)) = self.args.split_first() else {
            return Ok(ExecutionResult::success());
        };

        let builtin_name = name.to_string();
        let args = args.to_vec();

        if let Some(builtin) = context.shell.builtins().get(&builtin_name)
            && !builtin.is_disabled()
        {
            context.command_name = builtin_name;
            (builtin.execute_func())(context, args).await
        } else {
            Err(brush_core::ErrorKind::BuiltinNotFound(builtin_name).into())
        }
    }
}
