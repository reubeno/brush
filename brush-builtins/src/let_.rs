use std::io::Write;

use brush_core::{ExecutionExitCode, ExecutionResult, arithmetic::Evaluatable, builtins};

/// Evaluate arithmetic expressions.
pub(crate) struct LetCommand {
    /// Arithmetic expressions to evaluate.
    exprs: Vec<String>,
}

impl builtins::SpecCommand for LetCommand {
    type Error = brush_core::Error;

    fn declare(
        spec: builtins::argmodel::CommandSpecBuilder,
    ) -> builtins::argmodel::CommandSpecBuilder {
        spec
    }

    fn from_matches(
        matches: &mut builtins::argmodel::Matches,
    ) -> Result<Self, builtins::BuiltinArgParseError> {
        Ok(Self {
            exprs: matches.trailing().to_vec(),
        })
    }

    fn about() -> &'static str {
        "Evaluate arithmetic expressions."
    }

    fn synopsis() -> &'static str {
        "[EXPRESSION]..."
    }

    fn takes_trailing_args() -> bool {
        true
    }

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut result = ExecutionExitCode::InvalidUsage.into();

        if self.exprs.is_empty() {
            writeln!(context.stderr(), "missing expression")?;
            return Ok(result);
        }

        for expr in &self.exprs {
            let parsed = brush_parser::arithmetic::parse(expr.as_str())?;
            let evaluated = parsed.eval(context.shell)?;

            if evaluated == 0 {
                result = ExecutionResult::general_error();
            } else {
                result = ExecutionResult::success();
            }
        }

        Ok(result)
    }
}
