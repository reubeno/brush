use clap::Parser;
use std::io::Write;

use crate::{builtins, commands, error, tests, Shell};

/// Evaluate test expression.
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct TestCommand {
    #[clap(allow_hyphen_values = true)]
    args: Vec<String>,
}

impl builtins::Command for TestCommand {
    async fn execute(
        &self,
        context: commands::ExecutionContext<'_>,
    ) -> Result<crate::builtins::ExitCode, crate::error::Error> {
        let mut args = self.args.as_slice();

        if context.command_name == "[" {
            match args.last() {
                Some(s) if s == "]" => (),
                None | Some(_) => {
                    writeln!(context.stderr(), "[: missing ']'")?;
                    return Ok(builtins::ExitCode::InvalidUsage);
                }
            }

            args = &args[0..args.len() - 1];
        }

        if execute_test(context.shell, args)? {
            Ok(builtins::ExitCode::Success)
        } else {
            Ok(builtins::ExitCode::Custom(1))
        }
    }
}

fn execute_test(shell: &mut Shell, args: &[String]) -> Result<bool, error::Error> {
    let test_command =
        brush_parser::test_command::parse(args).map_err(error::Error::TestCommandParseError)?;
    tests::eval_test_expr(&test_command, shell)
}
