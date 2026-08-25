use std::io::Write;

use brush_core::{ExecutionResult, builtins};

#[derive(Debug, thiserror::Error)]
pub(crate) enum DirError {
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

/// Manage the current directory stack.
#[derive(Default)]
pub(crate) struct DirsCommand {
    clear: bool,
    tilde_long: bool,
    print_one_per_line: bool,
    print_one_per_line_with_index: bool,
}

const ID_CLEAR: &str = "clear";
const ID_TILDE_LONG: &str = "tilde_long";
const ID_PRINT_ONE_PER_LINE: &str = "print_one_per_line";
const ID_PRINT_ONE_PER_LINE_WITH_INDEX: &str = "print_one_per_line_with_index";

impl builtins::SpecCommand for DirsCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec.arg(
            ID_CLEAR,
            &['c'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Clear the directory stack.",
        )
        .arg(
            ID_TILDE_LONG,
            &['l'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Don't tilde-shorten paths.",
        )
        .arg(
            ID_PRINT_ONE_PER_LINE,
            &['p'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Print one directory per line instead of all on one line.",
        )
        .arg(
            ID_PRINT_ONE_PER_LINE_WITH_INDEX,
            &['v'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Print one directory per line with its index.",
        )
        // TODO(dirs): implement +N and -N
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            clear: matches.flag(ID_CLEAR),
            tilde_long: matches.flag(ID_TILDE_LONG),
            print_one_per_line: matches.flag(ID_PRINT_ONE_PER_LINE),
            print_one_per_line_with_index: matches.flag(ID_PRINT_ONE_PER_LINE_WITH_INDEX),
        })
    }

    fn about() -> &'static str {
        "Manage the current directory stack."
    }

    fn synopsis() -> &'static str {
        "[-clpv]"
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
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
