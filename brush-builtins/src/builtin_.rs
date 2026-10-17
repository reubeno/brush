use brush_core::{ExecutionResult, builtins};

/// Directly invokes a built-in, without going through typical search order.
pub(crate) struct BuiltinCommand {
    args: Vec<brush_core::CommandArg>,
}

impl builtins::ParseArgs for BuiltinCommand {
    const DECLARATION_ROLE: builtins::DeclarationRole = builtins::DeclarationRole::Utility;

    fn content(
        name: &str,
        content_type: builtins::ContentType,
        _options: &builtins::ContentOptions,
    ) -> Result<String, brush_core::Error> {
        match content_type {
            builtins::ContentType::DetailedHelp => Ok(
                "Execute SHELL-BUILTIN with arguments ARGs without performing command lookup.\n"
                    .into(),
            ),
            builtins::ContentType::ShortUsage => {
                Ok(format!("{name}: {name} [shell-builtin [arg ...]]\n"))
            }
            builtins::ContentType::ShortDescription => {
                Ok(format!("{name} - Execute shell builtins\n"))
            }
            builtins::ContentType::ManPage => {
                brush_core::error::unimp("man page not yet implemented")
            }
        }
    }

    fn parse(invocation: builtins::Invocation<'_>) -> Result<Self, builtins::UsageError> {
        Ok(Self {
            args: invocation.into_args(),
        })
    }
}

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
