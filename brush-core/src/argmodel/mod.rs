//! A backend-neutral description of built-in command arguments.
//!
//! See [`model`] for the description types and [`backend`] for the
//! compile-time backend selection.

mod backend;
mod model;

pub use model::{ArgKind, ArgSpec, CommandSpec, ParsedValues, PositionalSpec};

// N.B. Compiled only when bpaf is the *selected* backend; workspace feature
// unification can still link bpaf (the CLI uses it) without selecting it.
#[cfg(all(
    feature = "bpaf-linked",
    feature = "parser-bpaf",
    not(feature = "parser-usage"),
    not(feature = "parser-clap")
))]
mod bpaf_backend;
#[cfg(feature = "parser-clap")]
mod clap_backend;
#[cfg(feature = "parser-usage")]
mod usage_backend;

pub use backend::ArgParserBackend;

/// Returns the argument-parsing backend selected at compile time.
#[must_use]
pub fn backend() -> &'static dyn ArgParserBackend {
    backend::active()
}
#[cfg(test)]
mod backend_tests;
