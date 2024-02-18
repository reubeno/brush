use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

/// Change the current working directory.
#[derive(Parser, Debug)]
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
impl BuiltinCommand for CdCommand {
    async fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
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
                PathBuf::from(home_var)
            } else {
                log::error!("HOME not set");
                return Ok(BuiltinExitCode::Custom(1));
            }
        };

        match context.shell.set_working_dir(&target_dir) {
            Ok(()) => {}
            Err(e) => {
                log::error!("cd: {}", e);
                return Ok(BuiltinExitCode::Custom(1));
            }
        }

        Ok(BuiltinExitCode::Success)
    }
}
