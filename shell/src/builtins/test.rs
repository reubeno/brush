use clap::Parser;
use std::io::Write;

use crate::{
    builtin::{BuiltinCommand, BuiltinExitCode},
    error, tests, Shell,
};

#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
pub(crate) struct TestCommand {
    #[clap(allow_hyphen_values = true)]
    args: Vec<String>,
}

#[async_trait::async_trait]
impl BuiltinCommand for TestCommand {
    async fn execute(
        &self,
        context: crate::context::CommandExecutionContext<'_>,
    ) -> Result<crate::builtin::BuiltinExitCode, crate::error::Error> {
        let mut args = self.args.as_slice();

        if context.command_name == "[" {
            match args.last() {
                Some(s) if s == "]" => (),
                None | Some(_) => {
                    writeln!(context.stderr(), "[: missing ']'")?;
                    return Ok(BuiltinExitCode::InvalidUsage);
                }
            }

            args = &args[0..args.len() - 1];
        }

        if execute_test(context.shell, args)? {
            Ok(BuiltinExitCode::Success)
        } else {
            Ok(BuiltinExitCode::Custom(1))
        }
    }
}

fn execute_test(shell: &mut Shell, args: &[String]) -> Result<bool, error::Error> {
    let test_command =
        parser::parse_test_command(args).map_err(error::Error::TestCommandParseError)?;
    tests::eval_test_expr(&test_command, shell)
}
