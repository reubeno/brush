use clap::Parser;
use std::io::Write;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

/// Manage the current directory stack.
#[derive(Parser, Debug, Default)]
pub(crate) struct DirsCommand {
    /// Clear the directory stack.
    #[arg(short = 'c')]
    clear: bool,

    #[arg(short = 'l')]
    tilde_long: bool,

    /// Print one directory per line instead of all on one line.
    #[arg(short = 'p')]
    print_one_per_line: bool,

    #[arg(short = 'v')]
    print_one_per_line_with_index: bool,
    //
    // TODO: implement +N and -N
    //
}

#[async_trait::async_trait]
impl BuiltinCommand for DirsCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        if self.clear {
            context.shell.directory_stack.clear();
        } else {
            let dirs = vec![&context.shell.working_dir]
                .into_iter()
                .chain(context.shell.directory_stack.iter().rev())
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

            return Ok(BuiltinExitCode::Success);
        }

        Ok(BuiltinExitCode::Success)
    }
}
