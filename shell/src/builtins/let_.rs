use clap::Parser;
use std::io::Write;

use crate::{
    arithmetic::Evaluatable,
    builtin::{BuiltinCommand, BuiltinExitCode},
};

/// Evalute arithmetic expressions.
#[derive(Parser)]
pub(crate) struct LetCommand {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    exprs: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for LetCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        let mut exit_code = BuiltinExitCode::InvalidUsage;

        if self.exprs.is_empty() {
            writeln!(context.stderr(), "missing expression")?;
            return Ok(exit_code);
        }

        for expr in &self.exprs {
            let parsed = parser::parse_arithmetic_expression(expr.as_str())?;
            let evaluated = parsed.eval(context.shell).await?;

            if evaluated == 0 {
                exit_code = BuiltinExitCode::Custom(1);
            } else {
                exit_code = BuiltinExitCode::Custom(0);
            }
        }

        Ok(exit_code)
    }
}
