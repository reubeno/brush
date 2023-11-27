use anyhow::Result;
use clap::Parser;

use crate::shell::Shell;

#[derive(Debug)]
pub struct BuiltinResult {
    pub exit_code: BuiltinExitCode,
}

#[derive(Debug)]
pub enum BuiltinExitCode {
    Success,
    InvalidUsage,
    Unimplemented,
    Custom(u8),
    ExitShell(u8),
}

pub struct BuiltinExecutionContext<'a> {
    pub shell: &'a mut Shell,
    pub builtin_name: &'a str,
}

pub type BuiltinCommandExecuteFunc =
    fn(context: &mut BuiltinExecutionContext, args: &[&str]) -> Result<BuiltinResult>;

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
