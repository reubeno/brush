use brush_core::writeln_async;
use brush_core::{ExecutionResult, builtins};
use std::{borrow::Cow, path::Path};

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

    /// Operands, which are ignored. Options before the first operand are
    /// still misuse; everything after it is an operand.
    #[arg(positional, stop_flags, hide)]
    _operands: Vec<String>,
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

        writeln_async!(context.stdout_async(), "{}", cwd.to_string_lossy()).await?;

        Ok(ExecutionResult::success())
    }
}
