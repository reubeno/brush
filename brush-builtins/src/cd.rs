use std::io::Write;
use std::path::PathBuf;

use brush_core::{ExecutionResult, builtins, error};

/// Change the current shell working directory.
pub(crate) struct CdCommand {
    /// Exit with non zero exit status if current working directory resolution fails.
    exit_on_failed_cwd_resolution: bool,

    /// Show file with extended attributes as a dir with extended attributes.
    file_with_xattr_as_dir: bool,

    /// Whether an explicit physical/logical mode was requested; `Some(true)`
    /// means physical (`-P`) and `Some(false)` means logical (`-L`).
    mode: Option<bool>,

    /// By default it is the value of the HOME shell variable. If `TARGET_DIR` is "-", it is
    /// converted to $OLDPWD.
    target_dir: Option<PathBuf>,
}

const ID_EXIT_ON_FAILED_CWD_RESOLUTION: &str = "exit_on_failed_cwd_resolution";
const ID_FILE_WITH_XATTR_AS_DIR: &str = "file_with_xattr_as_dir";
const ID_PHYSICAL: &str = "physical";
const ID_LOGICAL: &str = "logical";
const ID_TARGET_DIR: &str = "target_dir";

impl builtins::SpecCommand for CdCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec.arg(
            ID_EXIT_ON_FAILED_CWD_RESOLUTION,
            &['e'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Exit with non zero exit status if current working directory resolution fails.",
        )
        .arg(
            ID_FILE_WITH_XATTR_AS_DIR,
            &['@'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Show file with extended attributes as a dir with extended attributes.",
        )
        .arg(
            ID_PHYSICAL,
            &['P'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Use physical dir structure without following symlinks.",
        )
        .arg(
            ID_LOGICAL,
            &['L'],
            &[],
            builtins::argmodel::ArgKind::Flag,
            None,
            "Force following symlinks.",
        )
        .positional(ID_TARGET_DIR, "TARGET_DIR")
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        // N.B. When both are supplied, physical wins; this preserves the old
        // parser's alternation order (`-P` listed before `-L`).
        let mode = if matches.flag(ID_PHYSICAL) {
            Some(true)
        } else if matches.flag(ID_LOGICAL) {
            Some(false)
        } else {
            None
        };

        Ok(Self {
            exit_on_failed_cwd_resolution: matches.flag(ID_EXIT_ON_FAILED_CWD_RESOLUTION),
            file_with_xattr_as_dir: matches.flag(ID_FILE_WITH_XATTR_AS_DIR),
            mode,
            target_dir: matches.value(ID_TARGET_DIR).map(PathBuf::from),
        })
    }

    fn about() -> &'static str {
        "Change the current shell working directory."
    }

    fn synopsis() -> &'static str {
        "[-LPe@] [TARGET_DIR]"
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, Self::Error> {
        // TODO(cd): implement 'cd -@'
        if self.file_with_xattr_as_dir {
            return error::unimp("cd -@");
        }

        let mut should_print = false;
        let mut target_dir = if let Some(target_dir) = &self.target_dir {
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

        if self.mode == Some(true)
            || context
                .shell
                .options()
                .do_not_resolve_symlinks_when_changing_dir
        {
            // -e is only relevant in physical mode.
            if self.exit_on_failed_cwd_resolution {
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
}
