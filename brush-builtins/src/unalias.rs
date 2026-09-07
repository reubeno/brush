//! The `unalias` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(UnaliasCommand);

use brush_core::ExecutionResult;
use std::io::Write;

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &UnaliasCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let mut exit_code = ExecutionResult::success();

    if command.remove_all {
        context.shell.aliases_mut().clear();
    } else {
        for alias in &command.aliases {
            if context.shell.aliases_mut().remove(alias).is_none() {
                writeln!(
                    context.stderr(),
                    "{}: {}: not found",
                    context.command_name,
                    alias
                )?;
                exit_code = ExecutionResult::general_error();
            }
        }
    }

    Ok(exit_code)
}
