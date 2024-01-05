use anyhow::Result;
use clap::Parser;
use futures::future::BoxFuture;

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
    pub builtin_name: String,
}

#[allow(clippy::module_name_repetitions)]
pub type BuiltinCommandExecuteFunc =
    fn(BuiltinExecutionContext<'_>, Vec<String>) -> BoxFuture<'_, Result<BuiltinResult>>;

#[allow(clippy::module_name_repetitions)]
#[async_trait::async_trait]
pub trait BuiltinCommand: Parser {
    async fn execute_args(
        mut context: BuiltinExecutionContext<'_>,
        args: Vec<String>,
    ) -> Result<BuiltinResult> {
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
            exit_code: parsed_args.execute(&mut context).await?,
        })
    }

    async fn execute(&self, context: &mut BuiltinExecutionContext<'_>) -> Result<BuiltinExitCode>;
}
