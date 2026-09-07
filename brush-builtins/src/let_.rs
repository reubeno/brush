//! The `let_` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(LetCommand);

use brush_core::{ExecutionExitCode, ExecutionResult, arithmetic::Evaluatable};
use std::io::Write;

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &LetCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let mut result = ExecutionExitCode::InvalidUsage.into();

    if command.exprs.is_empty() {
        writeln!(context.stderr(), "missing expression")?;
        return Ok(result);
    }

    for expr in &command.exprs {
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
