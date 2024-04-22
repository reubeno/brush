use clap::Parser;
use futures::future::BoxFuture;

use crate::commands::CommandArg;
use crate::context;
use crate::error;

#[macro_export]
macro_rules! minus_or_plus_flag_arg {
    ($struct_name:ident, $flag_char:literal, $desc:literal) => {
        #[derive(clap::Parser)]
        pub(crate) struct $struct_name {
            #[arg(short = $flag_char, name = concat!(stringify!($struct_name), "_enable"), action = clap::ArgAction::SetTrue, help = $desc)]
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
pub struct BuiltinResult {
    pub exit_code: BuiltinExitCode,
}

#[allow(clippy::module_name_repetitions)]
pub enum BuiltinExitCode {
    Success,
    InvalidUsage,
    Unimplemented,
    Custom(u8),
    ExitShell(u8),
    ReturnFromFunctionOrScript(u8),
    ContinueLoop(u8),
    BreakLoop(u8),
}

#[allow(clippy::module_name_repetitions)]
pub type BuiltinCommandExecuteFunc = fn(
    context::CommandExecutionContext<'_>,
    Vec<CommandArg>,
) -> BoxFuture<'_, Result<BuiltinResult, error::Error>>;

#[allow(clippy::module_name_repetitions)]
#[async_trait::async_trait]
pub trait BuiltinCommand: Parser {
    fn new<I>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = String>,
    {
        if !Self::takes_plus_options() {
            Self::try_parse_from(args)
        } else {
            // N.B. clap doesn't support named options like '+x'. To work around this, we
            // establish a pattern of renaming them.
            let args = args.into_iter().map(|arg| {
                if arg.starts_with('+') {
                    format!("--{arg}")
                } else {
                    arg
                }
            });

            Self::try_parse_from(args)
        }
    }

    fn takes_plus_options() -> bool {
        false
    }

    async fn execute(
        &self,
        context: context::CommandExecutionContext<'_>,
    ) -> Result<BuiltinExitCode, error::Error>;
}

#[allow(clippy::module_name_repetitions)]
#[async_trait::async_trait]
pub trait BuiltinDeclarationCommand: BuiltinCommand {
    fn set_declarations(&mut self, declarations: Vec<CommandArg>);
}
