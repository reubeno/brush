use std::io::Write;

use brush_core::{ExecutionExitCode, ExecutionResult, arithmetic::Evaluatable, builtins};

/// Evaluate arithmetic expressions.
#[derive(winnow_args::Args)]
#[arg(
    disable_help_short,
    disable_version_flag,
    disable_help_subcommand,
    unknown_flags = "value"
)]
pub(crate) struct LetCommand {
    /// Arithmetic expressions to evaluate.
    #[arg(positional, double_dash = "automatic", allow_negative_numbers)]
    exprs: Vec<String>,
}

brush_winnow_args_adapter::impl_parse_args!(LetCommand);

impl<SE: brush_core::ShellExtensions> builtins::Builtin<SE> for LetCommand {
    type Error = brush_core::Error;

    async fn execute(
        self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut result = ExecutionExitCode::InvalidUsage.into();

        if self.exprs.is_empty() {
            let mut stderr_output = Vec::new();
            writeln!(stderr_output, "missing expression")?;
            context.stderr().write_all(&stderr_output)?;
            context.stderr().flush()?;
            return Ok(result);
        }

        for expr in &self.exprs {
            let parsed = brush_parser::arithmetic::parse_with(
                expr.as_str(),
                context.shell.parser_options().parser_impl,
            )?;
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
