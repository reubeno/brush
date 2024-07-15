use std::io::Write;
use std::path::PathBuf;

use clap::Parser;

use crate::{builtins, commands};

/// Change the current working directory.
#[derive(Parser)]
pub(crate) struct CdCommand {
    /// Force following symlinks.
    #[arg(short = 'L')]
    force_follow_symlinks: bool,

    /// Use physical dir structure without following symlinks.
    #[arg(short = 'P')]
    use_physical_dir: bool,

    /// Exit if current working dir resolution fails.
    #[arg(short = 'e')]
    exit_on_failed_cwd_resolution: bool,

    /// Show file with extended attributes as a dir with extended
    /// attributes.
    #[arg(short = '@')]
    file_with_xattr_as_dir: bool,

    target_dir: Option<PathBuf>,
}

#[async_trait::async_trait]
impl builtins::Command for CdCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        // TODO: implement options
        if self.force_follow_symlinks
            || self.use_physical_dir
            || self.exit_on_failed_cwd_resolution
            || self.file_with_xattr_as_dir
        {
            return crate::error::unimp("options to cd");
        }

        let target_dir = if let Some(target_dir) = &self.target_dir {
            target_dir.clone()
        } else {
            if let Some(home_var) = context.shell.env.get_str("HOME") {
                PathBuf::from(home_var.to_string())
            } else {
                writeln!(context.stderr(), "HOME not set")?;
                return Ok(builtins::ExitCode::Custom(1));
            }
        };

        match context.shell.set_working_dir(&target_dir) {
            Ok(()) => {}
            Err(e) => {
                writeln!(context.stderr(), "cd: {e}")?;
                return Ok(builtins::ExitCode::Custom(1));
            }
        }

        Ok(builtins::ExitCode::Success)
    }
}
