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
        let name = invocation.name();
        let mut args = invocation.into_args();

        // `builtin` takes no options, but like bash it takes `--` and `--help`
        // and rejects anything else that looks like an option.
        let content = |content_type| {
            Self::content(name, content_type, &builtins::ContentOptions::default())
                .unwrap_or_default()
        };
        match args.first() {
            Some(brush_core::CommandArg::String(arg)) if arg == "--" => {
                args.remove(0);
            }
            Some(brush_core::CommandArg::String(arg)) if arg == "--help" => {
                let help = content(builtins::ContentType::DetailedHelp);
                return Err(builtins::UsageError::help(help.trim_end()));
            }
            Some(brush_core::CommandArg::String(arg)) if arg.len() > 1 && arg.starts_with('-') => {
                let usage = content(builtins::ContentType::ShortUsage);
                return Err(builtins::UsageError::new(format!(
                    "{name}: {arg}: invalid option\n{}",
                    usage.trim_end()
                )));
            }
            _ => (),
        }

        Ok(Self { args })
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

#[cfg(test)]
mod tests {
    use super::*;
    use builtins::{ContentOptions, ContentType, Invocation, ParseArgs};

    fn usage_error(args: &[&str]) -> builtins::UsageError {
        BuiltinCommand::parse(Invocation::new("builtin", args.iter().copied()))
            .err()
            .unwrap()
    }

    #[test]
    fn help_matches_detailed_help() {
        let err = usage_error(&["--help"]);
        assert!(err.is_help());
        let help = BuiltinCommand::content(
            "builtin",
            ContentType::DetailedHelp,
            &ContentOptions::default(),
        )
        .unwrap();
        assert_eq!(err.to_string(), help.trim_end());
    }

    #[test]
    fn invalid_option_shows_short_usage() {
        let err = usage_error(&["-z", "true"]);
        assert!(!err.is_help());
        let usage = BuiltinCommand::content(
            "builtin",
            ContentType::ShortUsage,
            &ContentOptions::default(),
        )
        .unwrap();
        assert_eq!(
            err.to_string(),
            format!("builtin: -z: invalid option\n{}", usage.trim_end())
        );
    }
}
