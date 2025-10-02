use brush_core::builtins;
use clap::Parser;
use std::{borrow::Cow, io::Write, path::Path};

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
    async fn execute(
        &self,
        context: brush_core::ExecutionContext<'_>,
    ) -> Result<brush_core::builtins::ExitCode, brush_core::Error> {
        let mut cwd: Cow<'_, Path> = context.shell.working_dir().into();

        let should_canonicalize = self.physical
            || context
                .shell
                .options
                .do_not_resolve_symlinks_when_changing_dir;

        if should_canonicalize {
            cwd = cwd.canonicalize()?.into();
        }

        writeln!(context.stdout(), "{}", cwd.to_string_lossy())?;

        Ok(builtins::ExitCode::Success)
    }
}
