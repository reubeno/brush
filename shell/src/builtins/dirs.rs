use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser, Debug, Default)]
pub(crate) struct DirsCommand {
    #[arg(short = 'c')]
    clear: bool,

    #[arg(short = 'l')]
    tilde_long: bool,

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
        context: &mut crate::builtin::BuiltinExecutionContext<'_>,
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
                    print!(" ");
                }

                if self.print_one_per_line_with_index {
                    print!("{i:2}  ");
                }

                let mut dir_str = dir.to_string_lossy().to_string();

                if !self.tilde_long {
                    dir_str = context.shell.tilde_shorten(dir_str);
                }

                print!("{dir_str}");

                if one_per_line || i == dirs.len() - 1 {
                    println!();
                }
            }

            return Ok(BuiltinExitCode::Success);
        }

        Ok(BuiltinExitCode::Success)
    }
}
