use clap::Parser;
use std::io::Write;

use crate::{arithmetic::Evaluatable, builtin, commands};

/// Evalute arithmetic expressions.
#[derive(Parser)]
pub(crate) struct LetCommand {
    /// Arithmetic expressions to evaluate.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    exprs: Vec<String>,
}

#[async_trait::async_trait]
impl builtin::Command for LetCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtin::ExitCode, crate::error::Error> {
        let mut exit_code = builtin::ExitCode::InvalidUsage;

        if self.exprs.is_empty() {
            writeln!(context.stderr(), "missing expression")?;
            return Ok(exit_code);
        }

        for expr in &self.exprs {
            let parsed = brush_parser::arithmetic::parse(expr.as_str())?;
            let evaluated = parsed.eval(context.shell).await?;

            if evaluated == 0 {
                exit_code = builtin::ExitCode::Custom(1);
            } else {
                exit_code = builtin::ExitCode::Custom(0);
            }
        }

        Ok(exit_code)
    }
}
