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
use brush_core::builtins::{self, builtin, decl_builtin, raw_arg_builtin, simple_builtin};

/// Returns the set of experimental built-in commands.
pub fn experimental_builtins<SE: brush_core::extensions::ShellExtensions>()
-> std::collections::HashMap<String, builtins::Registration<SE>> {
    let mut m = std::collections::HashMap::<String, builtins::Registration<SE>>::new();

    #[cfg(feature = "builtin.save")]
    m.insert("save".into(), builtin::<save::SaveCommand, SE>());

    m
}

/// Extension trait that simplifies adding experimental builtins to a shell builder.
pub trait ShellBuilderExt {
    /// Add experimental builtins to the shell being built.
    #[must_use]
    fn experimental_builtins(self) -> Self;
}

impl<SE: brush_core::extensions::ShellExtensions, S: brush_core::ShellBuilderState> ShellBuilderExt
    for brush_core::ShellBuilder<SE, S>
{
    fn experimental_builtins(self) -> Self {
        self.builtins(crate::experimental_builtins())
    }
}
