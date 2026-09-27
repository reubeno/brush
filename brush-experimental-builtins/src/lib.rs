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

use brush_core::builtins;

/// Returns the set of experimental built-in commands.
///
/// Saving a shell requires serializable installed policies; unsupported policy
/// types cannot register a save operation that silently discards their state.
#[cfg(feature = "builtin.save")]
pub fn experimental_builtins<SE: brush_core::extensions::ShellExtensions>()
-> std::collections::HashMap<String, builtins::Registration<SE>>
where
    brush_core::Shell<SE>: serde::Serialize,
{
    std::collections::HashMap::from([("save".into(), save::registration::<SE>())])
}

/// Returns no registrations when no experimental builtins are enabled.
#[cfg(not(feature = "builtin.save"))]
pub fn experimental_builtins<SE: brush_core::extensions::ShellExtensions>()
-> std::collections::HashMap<String, builtins::Registration<SE>> {
    std::collections::HashMap::new()
}

/// Extension trait that simplifies adding experimental builtins to a shell builder.
pub trait ShellBuilderExt {
    /// Add experimental builtins to the shell being built.
    #[must_use]
    fn experimental_builtins(self) -> Self;
}

#[cfg(feature = "builtin.save")]
impl<SE: brush_core::extensions::ShellExtensions, S: brush_core::ShellBuilderState> ShellBuilderExt
    for brush_core::ShellBuilder<SE, S>
where
    brush_core::Shell<SE>: serde::Serialize,
{
    fn experimental_builtins(self) -> Self {
        self.builtins(crate::experimental_builtins())
    }
}

#[cfg(not(feature = "builtin.save"))]
impl<SE: brush_core::extensions::ShellExtensions, S: brush_core::ShellBuilderState> ShellBuilderExt
    for brush_core::ShellBuilder<SE, S>
{
    fn experimental_builtins(self) -> Self {
        self
    }
}
