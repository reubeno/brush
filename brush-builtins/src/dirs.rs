use clap::Parser;
use std::io::Write;

use brush_core::{ExecutionResult, builtins};

/// What can go wrong in `dirs`, `pushd`, and `popd`, in their own terms.
#[derive(Debug, thiserror::Error)]
pub(crate) enum DirError {
    /// Directory stack is empty.
    #[error("directory stack is empty")]
    DirStackEmpty,
}

impl From<DirError> for brush_core::Error {
    fn from(error: DirError) -> Self {
        builtins::Failure::new(error).into()
    }
}

/// Manage the current directory stack.
#[derive(Default, Parser)]
pub(crate) struct DirsCommand {
    /// Clear the directory stack.
    #[arg(short = 'c')]
    clear: bool,

    /// Don't tilde-shorten paths.
    #[arg(short = 'l')]
    tilde_long: bool,

    /// Print one directory per line instead of all on one line.
    #[arg(short = 'p')]
    print_one_per_line: bool,

    /// Print one directory per line with its index.
    #[arg(short = 'v')]
    print_one_per_line_with_index: bool,
    //
    // TODO(dirs): implement +N and -N
}

brush_builtin_clap::impl_parse_args!(DirsCommand);

impl<SE: brush_core::ShellExtensions> builtins::StatelessBuiltin<SE> for DirsCommand {
    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, brush_core::Error> {
        if self.clear {
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

            let one_per_line = self.print_one_per_line || self.print_one_per_line_with_index;

            for (i, dir) in dirs.iter().enumerate() {
                if !one_per_line && i > 0 {
                    write!(context.stdout(), " ")?;
                }

                if self.print_one_per_line_with_index {
                    write!(context.stdout(), "{i:2}  ")?;
                }

                let mut dir_str = dir.to_string_lossy().to_string();

                if !self.tilde_long {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::{assert_eq, assert_matches};

    #[test]
    fn dir_errors_stay_dir_errors_in_the_shell_error() {
        let error = brush_core::Error::from(DirError::DirStackEmpty);
        assert_eq!(error.to_string(), "directory stack is empty");
        assert_eq!(u8::from(brush_core::ExecutionExitCode::from(&error)), 1);
        assert_matches!(
            error.kind(),
            brush_core::ErrorKind::BuiltinFailure(failure)
                if matches!(failure.downcast_ref::<DirError>(), Some(DirError::DirStackEmpty))
        );
    }
}
