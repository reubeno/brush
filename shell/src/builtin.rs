use anyhow::Result;
use clap::Parser;
use futures::future::BoxFuture;

use crate::shell::Shell;

#[macro_export]
macro_rules! minus_or_plus_flag_arg {
    ($struct_name:ident, $flag_char:literal, $desc:literal) => {
        #[derive(clap::Parser, Debug)]
        pub(crate) struct $struct_name {
            /// $desc
            #[arg(short = $flag_char, name = concat!(stringify!($struct_name), "_enable"), action = clap::ArgAction::SetTrue)]
            _enable: bool,
            #[arg(long = concat!("+", $flag_char), name = concat!(stringify!($struct_name), "_disable"), action = clap::ArgAction::SetTrue, hide = true)]
            _disable: bool,
        }

        impl From<$struct_name> for Option<bool> {
            fn from(value: $struct_name) -> Self {
                value.to_bool()
            }
        }

        impl $struct_name {
            #[allow(dead_code)]
            pub fn is_some(&self) -> bool {
                self._enable || self._disable
            }

            pub fn to_bool(&self) -> Option<bool> {
                match (self._enable, self._disable) {
                    (true, false) => Some(true),
                    (false, true) => Some(false),
                    _ => None,
                }
            }
        }
    };
}

pub(crate) use minus_or_plus_flag_arg;

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
        // N.B. clap doesn't support named options like '+x'. To work around this, we
        // establish a pattern of renaming them.
        let args: Vec<_> = args
            .into_iter()
            .map(|arg| {
                if arg.starts_with('+') {
                    format!("--{arg}")
                } else {
                    arg
                }
            })
            .collect();

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
