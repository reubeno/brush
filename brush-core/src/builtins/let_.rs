use clap::Parser;
use std::io::Write;

use crate::{arithmetic::Evaluatable, builtins, commands};

/// Evalute arithmetic expressions.
#[derive(Parser)]
pub(crate) struct LetCommand {
    /// Arithmetic expressions to evaluate.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    exprs: Vec<String>,
}

impl builtins::Command for LetCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        let mut exit_code = builtins::ExitCode::InvalidUsage;

        if self.exprs.is_empty() {
            writeln!(context.stderr(), "missing expression")?;
            return Ok(exit_code);
        }

        for expr in &self.exprs {
            let parsed = brush_parser::arithmetic::parse(expr.as_str())?;
            let evaluated = parsed.eval(context.shell).await?;

            if evaluated == 0 {
                exit_code = builtins::ExitCode::Custom(1);
            } else {
                exit_code = builtins::ExitCode::Custom(0);
            }
        }

        Ok(exit_code)
    }
}
