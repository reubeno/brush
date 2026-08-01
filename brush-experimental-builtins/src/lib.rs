//! Experimental builtins.

// See the equivalent note in `brush-builtins`: `brush_core::builtins::Command::execute` is async
// by contract (the trait declares a desugared `-> impl Future<...> + Send` so that the dispatch
// path can box every builtin uniformly), so a builtin that does purely synchronous work still
// implements `execute` as an `async fn` with no `.await`.
#![allow(
    clippy::unused_async_trait_impl,
    reason = "builtins implement a trait whose `execute` is async by contract"
)]

#[cfg(feature = "builtin.save")]
mod save;

#[allow(unused_imports, reason = "not all builtins are used in all configs")]
use brush_core::builtins::{self, builtin};

/// Registers experimental built-in commands on the given shell.
pub fn register_experimental_builtins<SE: brush_core::extensions::ShellExtensions>(
    shell: &mut brush_core::Shell<SE>,
) {
    #[cfg(feature = "builtin.save")]
    shell.register_builtin("save", builtin::<save::SaveCommand, SE>());
}
