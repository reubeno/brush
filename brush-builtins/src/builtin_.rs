use brush_core::argmodel::{CommandSpec, ParsedValues};
use brush_core::{ExecutionResult, builtins};

/// Directly invokes a built-in, without going through typical search order.
#[derive(Default)]
pub(crate) struct BuiltinCommand {
    args: Vec<brush_core::CommandArg>,
}

static BUILTIN_SPEC: CommandSpec = CommandSpec::EMPTY;

impl builtins::SpecCommand for BuiltinCommand {
    type Error = brush_core::Error;

    // N.B. Arguments are passed directly via `set_declarations`; spec() is
    // used only for help rendering.
    fn spec() -> &'static CommandSpec {
        &BUILTIN_SPEC
    }

    fn from_matches(_values: &mut ParsedValues) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self::default())
    }

    fn uses_declarations() -> bool {
        true
    }

    fn set_declarations(&mut self, args: Vec<brush_core::CommandArg>) {
        self.args = args;
    }

    fn about() -> &'static str {
        "Directly invokes a built-in, without going through typical search order."
    }

    fn synopsis() -> &'static str {
        "SHELL_BUILTIN [ARGS]..."
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        mut context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        if self.args.is_empty() {
            return Ok(ExecutionResult::success());
        }

        let args: Vec<_> = self.args.iter().skip(1).cloned().collect();
        if args.is_empty() {
            return Ok(ExecutionResult::success());
        }

        let builtin_name = args[0].to_string();

        if let Some(builtin) = context.shell.builtins().get(&builtin_name)
            && !builtin.disabled
        {
            context.command_name = builtin_name;
            (builtin.execute_func)(context, args).await
        } else {
            Err(brush_core::ErrorKind::BuiltinNotFound(builtin_name).into())
        }
    }
}
