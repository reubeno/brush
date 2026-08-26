//! The `test` builtin.

// N.B. Selects the engine-specific argument implementation; see `arg_impl!`.
arg_impl!(TestCommand);

use brush_core::{
    ErrorKind, ExecutionExitCode, ExecutionParameters, ExecutionResult, Shell, tests,
};
use std::io::Write;

pub(super) fn execute_test(
    shell: &mut Shell<impl brush_core::ShellExtensions>,
    params: &ExecutionParameters,
    args: &[String],
) -> Result<bool, brush_core::Error> {
    let test_command =
        brush_parser::test_command::parse(args).map_err(ErrorKind::TestCommandParseError)?;
    tests::eval_expr(&test_command, shell, params)
}

#[expect(clippy::unused_async, reason = "mirrors async trait contract")]
async fn execute<SE: brush_core::ShellExtensions>(
    command: &TestCommand,
    context: brush_core::ExecutionContext<'_, SE>,
) -> Result<brush_core::ExecutionResult, brush_core::Error> {
    let mut args = command.args.as_slice();

    if context.command_name == "[" {
        match args.last() {
            Some(s) if s == "]" => (),
            None | Some(_) => {
                writeln!(context.stderr(), "[: missing ']'")?;
                return Ok(ExecutionExitCode::InvalidUsage.into());
            }
        }

        args = &args[0..args.len() - 1];
    }

    if execute_test(context.shell, &context.params, args)? {
        Ok(ExecutionResult::success())
    } else {
        Ok(ExecutionResult::general_error())
    }
}
