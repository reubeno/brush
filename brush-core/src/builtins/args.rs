//! Engine-neutral contract for parsing builtin command arguments.
//!
//! A builtin's argument handling is expressed as a plain type that
//! implements [`FromArgs`]: given the name the builtin was invoked under and
//! the arguments that followed it, produce either an instance of the type or an
//! [`ArgsError`]. Nothing in this module names a particular argument-parsing
//! engine.
//!
//! The shell does no option handling on a builtin's behalf. It expands the
//! arguments and passes them through exactly as expanded; deciding which are
//! options, operands, or declarations is entirely up to the implementation.
//!
//! `brush-core` does not depend on any parsing engine. Builtins that use clap
//! get both this trait and [`super::HelpContent`] from the
//! `brush_builtin_utils::clap_builtin!` macro. Builtins that use anything else
//! implement the two traits directly:
//!
//! ```
//! use brush_core::CommandArg;
//! use brush_core::builtins::args::{ArgsError, FromArgs};
//!
//! struct ShiftArgs {
//!     n: Option<i32>,
//! }
//!
//! impl FromArgs for ShiftArgs {
//!     fn from_args(name: &str, args: Vec<CommandArg>) -> Result<Self, ArgsError> {
//!         match args.as_slice() {
//!             [] => Ok(Self { n: None }),
//!             [n] => n
//!                 .to_string()
//!                 .parse()
//!                 .map(|n| Self { n: Some(n) })
//!                 .map_err(|_| ArgsError::Usage(format!("{name}: numeric argument required"))),
//!             _ => Err(ArgsError::Usage(format!("{name}: too many arguments"))),
//!         }
//!     }
//! }
//! ```

use crate::CommandArg;

/// Error produced while parsing a builtin's arguments.
///
/// Distinguishes ordinary usage failures from help/version requests, so the
/// shell can present them differently. Both stop the builtin with the
/// invalid-usage exit code, and the shell currently reports both on stderr.
#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ArgsError {
    /// The arguments could not be parsed. The payload is the complete message
    /// to show the user, already rendered in the style the originating builtin
    /// prints.
    #[error("{0}")]
    Usage(String),

    /// The arguments were a request to display help (or version) information.
    /// The payload is the rendered text.
    #[error("{0}")]
    HelpRequested(String),
}

/// Contract for turning the arguments following a builtin's name into a typed
/// value.
pub trait FromArgs: Sized {
    /// Whether `name=value` operands reach [`FromArgs::from_args`] as
    /// [`CommandArg::Assignment`] rather than as strings.
    ///
    /// This is the one fact the shell needs before the builtin runs, because it
    /// changes how those words expand: as assignments, a tilde after `=` is
    /// expanded and the value is not split into fields (as in `export v=~/x`).
    const TAKES_DECLARATIONS: bool = false;

    /// Parses the given arguments into an instance of the implementing type.
    ///
    /// # Arguments
    ///
    /// * `name` - The name the builtin was invoked under. Implementations use
    ///   it to render usage and error text; it is never itself an argument.
    /// * `args` - The arguments as they appeared after expansion, excluding
    ///   `name`. Every one is a [`CommandArg::String`] unless
    ///   [`FromArgs::TAKES_DECLARATIONS`] is set.
    ///
    /// # Errors
    ///
    /// Returns an error if the arguments are invalid, or if they request help.
    fn from_args(name: &str, args: Vec<CommandArg>) -> Result<Self, ArgsError>;
}
