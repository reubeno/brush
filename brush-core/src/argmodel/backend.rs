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

/// Returns the backend selected at compile time.
#[must_use]
pub fn active() -> &'static dyn ArgParserBackend {
    // N.B. Priority when several backends are linked (cargo feature
    // unification across the workspace can pull more than one): dedicated
    // builtin backends win over bpaf, which is always linked anyway as the
    // shell CLI's own parser.
    #[cfg(feature = "parser-usage")]
    {
        &super::usage_backend::UsageBackend
    }
    #[cfg(all(not(feature = "parser-usage"), feature = "parser-clap"))]
    {
        &super::clap_backend::ClapBackend
    }
    #[cfg(all(
        not(feature = "parser-usage"),
        not(feature = "parser-clap"),
        feature = "parser-bpaf"
    ))]
    {
        &super::bpaf_backend::BpafBackend
    }
}
