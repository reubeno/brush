use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use crate::builtin::{BuiltinCommand, BuiltinExitCode};

#[derive(Parser, Debug)]
pub(crate) struct CdCommand {
    #[arg(short = 'L')]
    force_follow_symlinks: bool,

    #[arg(short = 'P')]
    use_physical_dir: bool,

    #[arg(short = 'e')]
    exit_on_failed_cwd_resolution: bool,

    #[arg(short = '@')]
    file_with_xattr_as_dir: bool,

    target_dir: Option<PathBuf>,
}

impl BuiltinCommand for CdCommand {
    fn execute(
        &self,
        context: &mut crate::builtin::BuiltinExecutionContext,
    ) -> Result<crate::builtin::BuiltinExitCode> {
        // TODO: implement options
        if self.force_follow_symlinks
            || self.use_physical_dir
            || self.exit_on_failed_cwd_resolution
            || self.file_with_xattr_as_dir
        {
            todo!("options to cd");
        }

        let target_path = if let Some(inner) = &self.target_dir {
            inner.clone()
        } else if let Some(home_var) = context.shell.variables.get("HOME") {
            PathBuf::from(String::from(&home_var.value))
        } else {
            log::error!("HOME not set");
            return Ok(BuiltinExitCode::Custom(1));
        };

        match std::fs::metadata(&target_path) {
            Ok(m) => {
                if !m.is_dir() {
                    log::error!("Not a directory");
                    return Ok(BuiltinExitCode::Custom(1));
                }
            }
            Err(e) => {
                log::error!("{}", e);
                return Ok(BuiltinExitCode::Custom(1));
            }
        }

        let pwd = target_path.to_string_lossy().to_string();

        // TODO: handle updating PWD
        context.shell.working_dir = target_path;
        context.shell.set_var("PWD", pwd.as_str(), true, false)?;

        Ok(BuiltinExitCode::Success)
    }
}
