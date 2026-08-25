use std::io::Write;

use brush_core::{ExecutionControlFlow, ExecutionExitCode, ExecutionResult, builtins};

/// Return from the current function.
pub(crate) struct ReturnCommand {
    code: Option<i32>,
}

const ID_CODE: &str = "code";

impl builtins::SpecCommand for ReturnCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec.positional(ID_CODE, "CODE")
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        let code = match matches.value(ID_CODE) {
            Some(value) => Some(value.parse().map_err(|_| builtins::BuiltinArgParseError {
                message: format!("invalid numeric value: {value}"),
                help_request: false,
            })?),
            None => None,
        };

        Ok(Self { code })
    }

    fn about() -> &'static str {
        "Return from the current function."
    }

    fn synopsis() -> &'static str {
        "[CODE]"
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        #[expect(clippy::cast_sign_loss)]
        let code_8bit = if let Some(code_32bit) = &self.code {
            (code_32bit & 0xFF) as u8
        } else {
            context.shell.last_exit_status()
        };

        if context.shell.in_function() || context.shell.in_sourced_script() {
            let mut result = ExecutionResult::new(code_8bit);
            result.next_control_flow = ExecutionControlFlow::ReturnFromFunctionOrScript;

            Ok(result)
        } else {
            let _ = writeln!(
                context.stderr(),
                "return: can only be used in a function or sourced script"
            );
            Ok(ExecutionExitCode::InvalidUsage.into())
        }
    }
}
