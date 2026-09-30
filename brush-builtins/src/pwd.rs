use brush_core::{ExecutionResult, builtins};
use std::{borrow::Cow, io::Write, path::Path};

/// Display the current working directory.
#[derive(winnow_args::Args)]
#[arg(disable_help_short, disable_version_flag, disable_help_subcommand)]
pub(crate) struct PwdCommand {
    /// Print $PWD if it names the current working directory.
    #[arg(short = 'L', overrides = "-P")]
    allow_symlinks: bool,

    /// Print the physical directory without any symlinks.
    #[arg(short = 'P', overrides = "-L")]
    physical: bool,
}

brush_builtin_winnow::winnow_builtin!(PwdCommand);

impl builtins::Command for PwdCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut cwd: Cow<'_, Path> = context.shell.working_dir().into();

        let should_canonicalize = self.physical
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
}
