//! The `alias` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(AliasCommand);

use brush_core::ExecutionResult;
use std::io::Write;

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &AliasCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let mut exit_code = ExecutionResult::success();

    if command.print || command.aliases.is_empty() {
        for (name, value) in context.shell.aliases() {
            writeln!(context.stdout(), "alias {name}='{value}'")?;
        }
    } else {
        for alias in &command.aliases {
            if let Some((name, unexpanded_value)) = alias.split_once('=')
                && !name.is_empty()
            {
                context
                    .shell
                    .aliases_mut()
                    .insert(name.to_owned(), unexpanded_value.to_owned());
            } else if let Some(value) = context.shell.aliases().get(alias) {
                writeln!(context.stdout(), "alias {alias}='{value}'")?;
            } else {
                writeln!(
                    context.stderr(),
                    "{}: {alias}: not found",
                    context.command_name
                )?;
                exit_code = ExecutionResult::general_error();
            }
        }
    }

    Ok(exit_code)
}
