use std::io::Write;

use brush_core::{ExecutionExitCode, ExecutionResult, arithmetic::Evaluatable, builtins};

/// Evaluate arithmetic expressions.
///
/// `let` has no options: `let -x=1` is an expression. One leading `--` is
/// still dropped.
#[derive(winnow_args::Args)]
#[arg(
    disable_help_short,
    disable_version_flag,
    disable_help_subcommand,
    unknown_flags = "value"
)]
pub(crate) struct LetCommand {
    /// Arithmetic expressions to evaluate.
    #[arg(
        positional,
        value_name = "arg",
        double_dash = "automatic",
        allow_negative_numbers
    )]
    exprs: Vec<String>,
}

brush_builtin_winnow::winnow_builtin!(LetCommand);

impl builtins::Command for LetCommand {
    type Error = brush_core::Error;

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
