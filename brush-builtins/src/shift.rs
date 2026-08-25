use brush_core::{ExecutionExitCode, ExecutionResult, argmodel::PositionalSpec, builtins};

/// Shift positional arguments.
pub(crate) struct ShiftCommand {
    n: Option<i32>,
}

const ID_N: &str = "n";

impl builtins::SpecCommand for ShiftCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static builtins::argmodel::CommandSpec {
        static SPEC: builtins::argmodel::CommandSpec = builtins::argmodel::CommandSpec {
            args: &[],
            positionals: &[PositionalSpec::one(ID_N, "N")],
        };

        &SPEC
    }

    fn from_matches(
        values: &mut builtins::argmodel::ParsedValues,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        let n = match values.value_of_positional(ID_N) {
            Some(value) => Some(value.parse().map_err(|_| builtins::BuiltinArgParseError {
                message: format!("invalid numeric value: {value}"),
                help_request: false,
            })?),
            None => None,
        };

        Ok(Self { n })
    }

    fn about() -> &'static str {
        "Shift positional arguments."
    }

    fn synopsis() -> &'static str {
        "[N]"
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let n = self.n.unwrap_or(1);

        if n < 0 {
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        #[expect(clippy::cast_sign_loss)]
        let n = n as usize;

        let args = context.shell.current_shell_args_mut();

        if n > args.len() {
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        args.drain(0..n);

        Ok(ExecutionResult::success())
    }
}
