//! The `builtin_` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(BuiltinCommand);

use brush_core::ExecutionResult;

async fn execute<SE: brush_core::ShellExtensions>(
    command: &BuiltinCommand,
    mut context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    if command.args.is_empty() {
        return Ok(ExecutionResult::success());
    }

    let args: Vec<_> = command.args.iter().skip(1).cloned().collect();
    if args.is_empty() {
        return Ok(ExecutionResult::success());
    }

    let builtin_name = args[0].to_string();

    if let Some(builtin) = context.shell.builtins().get(&builtin_name)
        && !builtin.disabled
    {
        context.command_name = builtin_name;
        (builtin.execute_func)(context, args).await
    } else {
        Err(brush_core::ErrorKind::BuiltinNotFound(builtin_name).into())
    }
}
