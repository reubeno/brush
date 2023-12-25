use anyhow::Result;
use clap::Parser;

use crate::shell::Shell;

#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct BuiltinResult {
    pub exit_code: BuiltinExitCode,
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub enum BuiltinExitCode {
    Success,
    InvalidUsage,
    Unimplemented,
    Custom(u8),
    ExitShell(u8),
    ReturnFromFunctionOrScript(u8),
}

#[allow(clippy::module_name_repetitions)]
pub struct BuiltinExecutionContext<'a> {
    pub shell: &'a mut Shell,
    pub builtin_name: &'a str,
}

#[allow(clippy::module_name_repetitions)]
pub type BuiltinCommandExecuteFunc =
    fn(context: &mut BuiltinExecutionContext, args: &[&str]) -> Result<BuiltinResult>;

#[allow(clippy::module_name_repetitions)]
pub trait BuiltinCommand: Parser {
    fn execute_args(context: &mut BuiltinExecutionContext, args: &[&str]) -> Result<BuiltinResult> {
        let parse_result = Self::try_parse_from(args);
        let parsed_args = match parse_result {
            Ok(parsed_args) => parsed_args,
            Err(e) => {
                log::error!("{}", e);
                return Ok(BuiltinResult {
                    exit_code: BuiltinExitCode::InvalidUsage,
                });
            }
        };

        Ok(BuiltinResult {
            exit_code: parsed_args.execute(context)?,
        })
    }

    fn execute(&self, context: &mut BuiltinExecutionContext) -> Result<BuiltinExitCode>;
}
