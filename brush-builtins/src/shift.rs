use brush_core::{ExecutionExitCode, ExecutionResult, builtins};

/// Shift positional arguments.
pub(crate) struct ShiftCommand {
    n: Option<i32>,
}

const ID_N: &str = "n";

impl builtins::SpecCommand for ShiftCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec.positional(ID_N, "N")
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        let n = match matches.value(ID_N) {
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
