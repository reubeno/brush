//! Compile-time selection of the argument-parsing backend.
//!
//! Exactly one of the `parser-bpaf`, `parser-usage`, or `parser-clap` features
//! must be enabled. The chosen backend turns a backend-neutral
//! [`CommandSpec`](super::CommandSpec) into an actual parser and maps its
//! results back into [`Matches`](super::Matches); nothing else in brush knows
//! which crate is in play.

use super::{CommandSpec, ParsedValues};
use crate::builtins::BuiltinArgParseError;

/// A parser backend for the built-in argument model.
pub trait ArgParserBackend: Sync {
    /// Parses `argv` (which does *not* include the command name) against
    /// `spec`.
    ///
    /// # Arguments
    ///
    /// * `spec` - The declared argument surface.
    /// * `name` - The builtin's invocation name, used for help rendering.
    /// * `argv` - The words to parse.
    fn parse(
        &self,
        spec: &'static CommandSpec,
        name: &str,
        argv: &[String],
    ) -> Result<ParsedValues, BuiltinArgParseError>;

    /// Renders the detailed help page for `spec`.
    fn detailed_help(
        &self,
        spec: &'static CommandSpec,
        name: &str,
    ) -> Result<String, crate::error::Error>;
}

#[cfg(all(feature = "parser-bpaf", feature = "parser-usage"))]
compile_error!("only one parser backend feature may be enabled at a time");
#[cfg(all(feature = "parser-bpaf", feature = "parser-clap"))]
compile_error!("only one parser backend feature may be enabled at a time");
#[cfg(all(feature = "parser-usage", feature = "parser-clap"))]
compile_error!("only one parser backend feature may be enabled at a time");
#[cfg(not(any(
    feature = "parser-bpaf",
    feature = "parser-usage",
    feature = "parser-clap"
)))]
compile_error!("one parser backend feature must be enabled");

/// Returns the backend selected at compile time.
#[must_use]
pub fn active() -> &'static dyn ArgParserBackend {
    #[cfg(feature = "parser-bpaf")]
    {
        &super::bpaf_backend::BpafBackend
    }
    #[cfg(feature = "parser-usage")]
    {
        &super::usage_backend::UsageBackend
    }
    #[cfg(feature = "parser-clap")]
    {
        &super::clap_backend::ClapBackend
    }
}
