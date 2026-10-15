use brush_core::builtins::{self, Help, Invocation, ParseArgs, UsageError};
use brush_core::{CommandArg, ExecutionResult};

/// Runs a builtin, without looking for a function of the same name.
pub(crate) struct BuiltinCommand {
    /// The builtin's name, then its arguments, as they arrived.
    args: Vec<CommandArg>,
}

impl ParseArgs for BuiltinCommand {
    fn help(name: &str) -> Help {
        Help::new(
            format!("{name} [shell-builtin [arg ...]]"),
            "Execute shell builtins.",
        )
        .with_details(
            "Execute SHELL-BUILTIN with arguments ARGs without performing command\n\
             lookup.  This is useful when you wish to reimplement a shell builtin\n\
             as a shell function, but need to execute the builtin within the function.\n\
             \n\
             Exit Status:\n\
             Returns the exit status of SHELL-BUILTIN, or false if SHELL-BUILTIN is\n\
             not a shell builtin.",
        )
    }

    /// `builtin` has no options of its own, as in bash: `--help` asks for its
    /// help, `--` ends its options, and any other word that looks like an
    /// option is an invalid one. What follows is the builtin to run and its
    /// arguments, untouched.
    fn parse(invocation: Invocation<'_>) -> Result<Self, UsageError> {
        let mut args = invocation.into_args();
        if let Some(CommandArg::String(first)) = args.first() {
            match first.as_str() {
                "--help" => return Err(UsageError::help_requested()),
                "--" => {
                    args.remove(0);
                }
                option if option.len() > 1 && option.starts_with('-') => {
                    // Named as bash names it: `-x` for `-xy`, and `--` for `--x`.
                    let shown: String = option.chars().take(2).collect();
                    return Err(UsageError::invalid_option(shown));
                }
                _ => {}
            }
        }
        Ok(Self { args })
    }
}

impl<SE: brush_core::ShellExtensions> builtins::StatelessBuiltin<SE> for BuiltinCommand {
    // Unlike bash's, brush's `builtin` is a declaration utility, so that
    // `builtin export x=~/y` still expands its `~`: brush doesn't yet expand a
    // `~` after the `=` of an ordinary argument, as bash does.
    const DECLARATIONS: builtins::Declarations = builtins::Declarations::Yes;

    async fn execute(
        self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<ExecutionResult, brush_core::Error> {
        context.run_builtin(self.args).await
    }
}
