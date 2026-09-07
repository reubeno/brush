//! The `cd` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(CdCommand);

use brush_core::{ExecutionResult, error};
use std::io::Write;
use std::path::PathBuf;

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &CdCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    // TODO(cd): implement 'cd -@'
    if command.file_with_xattr_as_dir {
        return error::unimp("cd -@");
    }

    let mut should_print = false;
    let mut target_dir = if let Some(target_dir) = &command.target_dir {
        // `cd -', equivalent to `cd $OLDPWD'
        if target_dir.as_os_str() == "-" {
            should_print = true;
            if let Some(oldpwd) = context.shell.env_str("OLDPWD") {
                PathBuf::from(oldpwd.to_string())
            } else {
                writeln!(context.stderr(), "OLDPWD not set")?;
                return Ok(ExecutionResult::general_error());
            }
        } else {
            // TODO(cd): remove clone, and use temporary lifetime extension after rust 1.75
            target_dir.clone()
        }
    // `cd' without arguments is equivalent to `cd $HOME'
    } else {
        if let Some(home_var) = context.shell.env_str("HOME") {
            PathBuf::from(home_var.to_string())
        } else {
            writeln!(context.stderr(), "HOME not set")?;
            return Ok(ExecutionResult::general_error());
        }
    };

    if command.use_physical_dir
        || context
            .shell
            .options()
            .do_not_resolve_symlinks_when_changing_dir
    {
        // -e is only relevant in physical mode.
        if command.exit_on_failed_cwd_resolution {
            return error::unimp("cd -e");
        }

        target_dir = context.shell.absolute_path(target_dir).canonicalize()?;
    }

    context.shell.set_working_dir(&target_dir)?;

    // Bash compatibility
    // https://www.gnu.org/software/bash/manual/bash.html#index-cd
    // If a non-empty directory name from CDPATH is used, or if '-' is the first argument, and
    // the directory change is successful, the absolute pathname of the new working
    // directory is written to the standard output.
    if should_print {
        writeln!(context.stdout(), "{}", target_dir.display())?;
    }

    Ok(ExecutionResult::success())
}
