use std::io::Write;

use brush_core::{
    ErrorKind, ExecutionExitCode, ExecutionParameters, ExecutionResult, Shell, builtins, tests,
};

/// Evaluate test expression.
///
/// Every word is the expression's, `--`, `-o` and a first `--help` included.
#[derive(winnow_args::Args)]
#[arg(
    unknown_flags = "value",
    disable_help_flag,
    disable_version_flag,
    disable_help_subcommand
)]
pub(crate) struct TestCommand {
    #[arg(positional, double_dash = "preserve")]
    args: Vec<String>,
}

brush_builtin_winnow::winnow_builtin!(TestCommand, trailing_args = args);

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
