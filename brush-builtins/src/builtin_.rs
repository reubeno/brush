use brush_core::{ExecutionResult, builtins};

/// Execute SHELL-BUILTIN with arguments ARGs without performing command lookup.
#[derive(Default, winnow_args::Args)]
#[arg(
    usage = "builtin [shell-builtin [arg ...]]",
    disable_help_short,
    disable_version_flag,
    disable_help_subcommand
)]
pub(crate) struct BuiltinCommand {
    // The builtin to run and its arguments, as they arrived. Skipped by winnow-args,
    // and filled in by `impl_parse_args!`.
    #[arg(skip)]
    args: Vec<brush_core::CommandArg>,
}

brush_winnow_args_adapter::impl_parse_args!(BuiltinCommand, declarations = args);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for BuiltinCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let args = self.args;
        if args.is_empty() {
            return Ok(ExecutionResult::success());
        }

        let builtin_name = args[0].to_string();

        if let Some(builtin) = context
            .shell
            .builtins()
            .get(&builtin_name)
            .filter(|builtin| builtin.is_enabled())
            .cloned()
        {
            context.command_name = builtin_name;
            builtin.execute(context, args).await
        } else {
            Err(brush_core::ErrorKind::BuiltinNotFound(builtin_name).into())
        }
    }
}
