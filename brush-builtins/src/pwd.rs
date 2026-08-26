//! The `pwd` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(PwdCommand);

use brush_core::ExecutionResult;
use std::{borrow::Cow, io::Write, path::Path};

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &PwdCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let mut cwd: Cow<'_, Path> = context.shell.working_dir().into();

    let should_canonicalize = command.physical
        || context
            .shell
            .options()
            .do_not_resolve_symlinks_when_changing_dir;

    if should_canonicalize {
        cwd = cwd.canonicalize()?.into();
    }

    writeln!(context.stdout(), "{}", cwd.to_string_lossy())?;

    Ok(ExecutionResult::success())
}
