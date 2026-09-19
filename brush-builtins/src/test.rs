use clap::Parser;
use std::io::Write;

use brush_core::{
    ErrorKind, ExecutionExitCode, ExecutionParameters, ExecutionResult, Shell, builtins, tests,
};

/// Evaluate test expression.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct TestCommand {
    #[clap(allow_hyphen_values = true)]
    args: Vec<String>,
}

brush_builtin_utils::clap_builtin!(TestCommand, trailing_args = args);

impl builtins::Command for TestCommand {
    type Error = brush_core::Error;

    async fn execute<SE: brush_core::ShellExtensions>(
        &self,
        context: brush_core::ExecutionContext<'_, SE>,
    ) -> Result<brush_core::ExecutionResult, Self::Error> {
        let mut args = self.args.as_slice();

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
}

fn execute_test(
    shell: &mut Shell<impl brush_core::ShellExtensions>,
    params: &ExecutionParameters,
    args: &[String],
) -> Result<bool, brush_core::Error> {
    let test_command =
        brush_parser::test_command::parse(args).map_err(ErrorKind::TestCommandParseError)?;
    tests::eval_expr(&test_command, shell, params)
}
