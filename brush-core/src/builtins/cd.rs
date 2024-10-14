use std::io::Write;
use std::path::PathBuf;

use clap::Parser;

use crate::{builtins, commands};

/// Change the current shell working directory.
#[derive(Parser)]
pub(crate) struct CdCommand {
    /// Force following symlinks.
    #[arg(short = 'L')]
    force_follow_symlinks: bool,

    /// Use physical dir structure without following symlinks.
    #[arg(short = 'P')]
    use_physical_dir: bool,

    /// Exit with non zero exit status if current working directory resolution fails.
    #[arg(short = 'e')]
    exit_on_failed_cwd_resolution: bool,

    /// Show file with extended attributes as a dir with extended
    /// attributes.
    #[arg(short = '@')]
    file_with_xattr_as_dir: bool,

    /// By default it is the value of the HOME shell variable. If `TARGET_DIR` is "-", it is
    /// converted to $OLDPWD.
    target_dir: Option<PathBuf>,
}

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

        let mut should_print = false;
        let target_dir = if let Some(target_dir) = &self.target_dir {
            // `cd -', equivalent to `cd $OLDPWD'
            if target_dir.as_os_str() == "-" {
                should_print = true;
                if let Some(oldpwd) = context.shell.env.get_str("OLDPWD") {
                    PathBuf::from(oldpwd.to_string())
                } else {
                    writeln!(context.stderr(), "OLDPWD not set")?;
                    return Ok(builtins::ExitCode::Custom(1));
                }
            } else {
                // TODO: remove clone, and use temporary lifetime extension after rust 1.75
                target_dir.clone()
            }
        // `cd' without arguments is equivalent to `cd $HOME'
        } else {
            if let Some(home_var) = context.shell.env.get_str("HOME") {
                PathBuf::from(home_var.to_string())
            } else {
                writeln!(context.stderr(), "HOME not set")?;
                return Ok(builtins::ExitCode::Custom(1));
            }
        };

        if let Err(e) = context.shell.set_working_dir(&target_dir) {
            writeln!(context.stderr(), "cd: {e}")?;
            return Ok(builtins::ExitCode::Custom(1));
        }

        // Bash compatibility
        // https://www.gnu.org/software/bash/manual/bash.html#index-cd
        // If a non-empty directory name from CDPATH is used, or if '-' is the first argument, and
        // the directory change is successful, the absolute pathname of the new working
        // directory is written to the standard output.
        if should_print {
            writeln!(context.stdout(), "{}", target_dir.display())?;
        }

        Ok(builtins::ExitCode::Success)
    }
}
