use brush_core::{
    ExecutionControlFlow, ExecutionExitCode, ExecutionResult, argmodel::PositionalSpec, builtins,
};

/// Breaks out of a control-flow loop.
pub(crate) struct BreakCommand {
    which_loop: i8,
}

const ID_WHICH_LOOP: &str = "which_loop";

impl builtins::SpecCommand for BreakCommand {
    type Error = brush_core::Error;

    fn spec() -> &'static builtins::argmodel::CommandSpec {
        static SPEC: builtins::argmodel::CommandSpec = builtins::argmodel::CommandSpec {
            args: &[],
            positionals: &[PositionalSpec::one(ID_WHICH_LOOP, "WHICH_LOOP")],
        };

        &SPEC
    }

    fn from_matches(
        values: &mut builtins::argmodel::ParsedValues,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        let which_loop = match values.value_of_positional(ID_WHICH_LOOP) {
            Some(value) => value.parse().map_err(|_| builtins::BuiltinArgParseError {
                message: format!("invalid numeric value: {value}"),
                help_request: false,
            })?,
            None => 1,
        };

        Ok(Self { which_loop })
    }

    fn about() -> &'static str {
        "Breaks out of a control-flow loop."
    }

    fn synopsis() -> &'static str {
        "[N]"
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        _context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        // If specified, which_loop needs to be positive.
        if self.which_loop <= 0 {
            return Ok(ExecutionExitCode::InvalidUsage.into());
        }

        let mut result = ExecutionResult::success();

        result.next_control_flow = ExecutionControlFlow::BreakLoop {
            #[expect(clippy::cast_sign_loss)]
            levels: (self.which_loop - 1) as usize,
        };

        Ok(result)
    }
}
