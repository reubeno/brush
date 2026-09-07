//! The `times` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(TimesCommand);

use brush_core::{ExecutionResult, timing};
use std::io::Write;

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    _command: &TimesCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let (self_user, self_system) = brush_core::sys::resource::get_self_user_and_system_time()?;
    writeln!(
        context.stdout(),
        "{} {}",
        timing::format_duration_non_posixly(&self_user),
        timing::format_duration_non_posixly(&self_system),
    )?;

    let (children_user, children_system) =
        brush_core::sys::resource::get_children_user_and_system_time()?;
    writeln!(
        context.stdout(),
        "{} {}",
        timing::format_duration_non_posixly(&children_user),
        timing::format_duration_non_posixly(&children_system),
    )?;

    Ok(ExecutionResult::success())
}
