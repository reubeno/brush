use std::io::Write;
use std::path::PathBuf;

use clap::Parser;

use brush_core::{ExecutionResult, builtins, error};

/// Change the current shell working directory.
#[derive(Parser)]
pub(crate) struct CdCommand {
    /// Force following symlinks.
    #[arg(short = 'L', overrides_with = "use_physical_dir")]
    force_follow_symlinks: bool,

    /// Use physical dir structure without following symlinks.
    #[arg(short = 'P', overrides_with = "force_follow_symlinks")]
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
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        if self.file_with_xattr_as_dir {
            return error::unimp("cd -@");
        }

        let mut should_print = false;
        let mut target_dir = if let Some(target_dir) = &self.target_dir {
            if target_dir.as_os_str() == "-" {
                should_print = true;
                if let Some(oldpwd) = context.shell.env_str("OLDPWD") {
                    PathBuf::from(oldpwd.to_string())
                } else {
                    writeln!(context.stderr(), "OLDPWD not set")?;
                    return Ok(ExecutionResult::general_error());
                }
            } else {
                target_dir.clone()
            }
        } else {
            if let Some(home_var) = context.shell.env_str("HOME") {
                PathBuf::from(home_var.to_string())
            } else {
                writeln!(context.stderr(), "HOME not set")?;
                return Ok(ExecutionResult::general_error());
            }
        };

        if self.use_physical_dir
            || context
                .shell
                .options()
                .do_not_resolve_symlinks_when_changing_dir
        {
            if self.exit_on_failed_cwd_resolution {
                return error::unimp("cd -e");
            }

            target_dir = context.shell.absolute_path(target_dir).canonicalize()?;
        }

        context.shell.set_working_dir(&target_dir)?;

        if should_print {
            let mut output = Vec::new();
            writeln!(output, "{}", target_dir.display())?;
            if let Some(mut stdout) = context.stdout_async() {
                stdout.write_all(&output).await?;
                stdout.flush().await?;
            } else {
                context.stdout().write_all(&output)?;
                context.stdout().flush()?;
            }
        }

        Ok(ExecutionResult::success())
    }
}
