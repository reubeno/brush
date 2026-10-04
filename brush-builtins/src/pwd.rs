use brush_core::{ExecutionResult, builtins};
use clap::Parser;
use std::{borrow::Cow, io::Write};

/// Display the current working directory.
#[derive(Parser)]
pub(crate) struct PwdCommand {
    /// Print the physical directory without any symlinks.
    #[arg(short = 'P', overrides_with = "allow_symlinks")]
    physical: bool,

    /// Print $PWD if it names the current working directory.
    #[arg(short = 'L', overrides_with = "physical")]
    allow_symlinks: bool,
}

impl builtins::Command for PwdCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut cwd = Cow::Borrowed(context.shell.working_dir());

        // The shell started somewhere it couldn't find (e.g., a deleted directory).
        if cwd.is_empty() {
            writeln!(
                context.stderr(),
                "{}: error retrieving current directory",
                context.command_name
            )?;
            return Ok(ExecutionResult::general_error());
        }

        let should_canonicalize = self.physical
            || context
                .shell
                .options()
                .do_not_resolve_symlinks_when_changing_dir;

        if should_canonicalize {
            cwd = Cow::Owned(cwd.canonicalize()?);
        }

        writeln!(context.stdout(), "{}", cwd.as_path().to_string_lossy())?;

        Ok(ExecutionResult::success())
    }
}
