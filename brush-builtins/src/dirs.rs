//! The `dirs` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(DirsCommand);

use brush_core::ExecutionResult;
use std::io::Write;

#[derive(Debug, thiserror::Error)]
pub(super) enum DirError {
    /// Directory stack is empty.
    #[error("directory stack is empty")]
    DirStackEmpty,

    /// A shell error occurred.
    #[error(transparent)]
    ShellError(#[from] brush_core::Error),
}

impl From<&DirError> for brush_core::ExecutionExitCode {
    fn from(value: &DirError) -> Self {
        match value {
            DirError::DirStackEmpty => Self::GeneralError,
            DirError::ShellError(e) => e.into(),
        }
    }
}

impl brush_core::BuiltinError for DirError {}

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &DirsCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    if command.clear {
        context.shell.directory_stack_mut().clear();
    } else {
        let dirs = vec![context.shell.working_dir()]
            .into_iter()
            .chain(
                context
                    .shell
                    .directory_stack()
                    .iter()
                    .rev()
                    .map(|p| p.as_path()),
            )
            .collect::<Vec<_>>();

        let one_per_line = command.print_one_per_line || command.print_one_per_line_with_index;

        for (i, dir) in dirs.iter().enumerate() {
            if !one_per_line && i > 0 {
                write!(context.stdout(), " ")?;
            }

            if command.print_one_per_line_with_index {
                write!(context.stdout(), "{i:2}  ")?;
            }

            let mut dir_str = dir.to_string_lossy().to_string();

            if !command.tilde_long {
                dir_str = context.shell.tilde_shorten(dir_str);
            }

            write!(context.stdout(), "{dir_str}")?;

            if one_per_line || i == dirs.len() - 1 {
                writeln!(context.stdout())?;
            }
        }

        return Ok(ExecutionResult::success());
    }

    Ok(ExecutionResult::success())
}
