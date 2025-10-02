use clap::Parser;
use std::io::Write;

use brush_core::builtins;

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
    // TODO: implement +N and -N
}

impl builtins::Command for DirsCommand {
    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::builtins::ExitCode, brush_core::Error> {
        if self.clear {
            context.shell.directory_stack.clear();
        } else {
            let dirs = vec![context.shell.working_dir()]
                .into_iter()
                .chain(
                    context
                        .shell
                        .directory_stack
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

            return Ok(builtins::ExitCode::Success);
        }

        Ok(builtins::ExitCode::Success)
    }
}
