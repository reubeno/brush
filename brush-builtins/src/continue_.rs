use brush_core::{ExecutionControlFlow, ExecutionExitCode, ExecutionResult, builtins};

/// Continue to the next iteration of a control-flow loop.
pub(crate) struct ContinueCommand {
    which_loop: i8,
}

const ID_WHICH_LOOP: &str = "which_loop";

impl builtins::SpecCommand for ContinueCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec.positional(ID_WHICH_LOOP, "WHICH_LOOP")
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        let which_loop = match matches.value(ID_WHICH_LOOP) {
            Some(value) => value.parse().map_err(|_| builtins::BuiltinArgParseError {
                message: format!("invalid numeric value: {value}"),
                help_request: false,
            })?,
            None => 1,
        };

        Ok(Self { which_loop })
    }

    fn about() -> &'static str {
        "Continue to the next iteration of a control-flow loop."
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

        result.next_control_flow = ExecutionControlFlow::ContinueLoop {
            #[expect(clippy::cast_sign_loss)]
            levels: (self.which_loop - 1) as usize,
        };

        Ok(result)
    }
}
