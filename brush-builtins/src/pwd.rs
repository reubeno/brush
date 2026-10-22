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

brush_winnow_args_adapter::impl_parse_args!(PwdCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for PwdCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
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

        let mut output = Vec::new();
        writeln!(output, "{}", cwd.to_string_lossy())?;

        if let Some(mut stdout) = context.stdout_async() {
            stdout.write_all(&output).await?;
            stdout.flush().await?;
        } else {
            context.stdout().write_all(&output)?;
            context.stdout().flush()?;
        }

        Ok(ExecutionResult::success())
    }
}
