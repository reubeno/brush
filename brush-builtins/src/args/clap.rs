//! The clap-backed argument parsing for builtins.
//!
//! Transitional module: owns word-to-struct binding and (for now) help
//! rendering for converted builtins. When brush gains an engine-neutral help
//! model, the mirror types here shrink to pure binding code.

use brush_core::args::{ArgsError, FromArgs};
use clap::Parser;

use crate::echo::EchoCommand;
use brush_core::builtins::{ContentOptions, ContentType, clap_content};

/// Renders a converted builtin's help from its mirror type's metadata.
pub fn help<M: Parser>(
    name: &str,
    content_type: &ContentType,
    options: &ContentOptions,
) -> Result<String, brush_core::error::Error> {
    clap_content::<M>(name, content_type, options)
}

/// Mirror of [`EchoCommand`]'s arguments carrying the engine metadata
/// (option definitions and help text).
#[derive(Parser)]
#[clap(disable_help_flag = true, disable_version_flag = true)]
struct EchoHelp {
    /// Suppress the trailing newline from the output.
    #[arg(short = 'n')]
    no_trailing_newline: bool,

    /// Interpret backslash escapes in the provided text.
    #[arg(short = 'e')]
    interpret_backslash_escapes: bool,

    /// Do not interpret backslash escapes in the provided text.
    #[arg(short = 'E')]
    no_interpret_backslash_escapes: bool,

    /// Tokens to echo to standard output.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

impl From<EchoHelp> for EchoCommand {
    fn from(help: EchoHelp) -> Self {
        Self {
            no_trailing_newline: help.no_trailing_newline,
            interpret_backslash_escapes: help.interpret_backslash_escapes,
            no_interpret_backslash_escapes: help.no_interpret_backslash_escapes,
            args: help.args,
        }
    }
}

impl FromArgs for EchoCommand {
    fn from_args(words: &[String]) -> Result<Self, ArgsError> {
        // N.B. clap's handling of '--' differs from bash's (see
        // [`brush_core::builtins::parse_known`]): parse everything before the
        // first '--', then append '--' and the remainder verbatim so echo
        // prints them like bash does.
        let (mut parsed, rest_args) =
            brush_core::builtins::try_parse_known::<EchoHelp>(words.to_vec())
                .map_err(|err| ArgsError::from_clap_error(&err))?;
        if let Some(rest) = rest_args {
            parsed.args.extend(rest);
        }

        Ok(Self::from(parsed))
    }
}

/// Returns help content for `echo`.
pub fn echo_help(
    name: &str,
    content_type: &ContentType,
    options: &ContentOptions,
) -> Result<String, brush_core::error::Error> {
    help::<EchoHelp>(name, content_type, options)
}
