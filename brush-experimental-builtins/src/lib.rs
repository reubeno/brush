//! Experimental builtins.

#[cfg(feature = "builtin.save")]
mod save;

#[allow(unused_imports, reason = "not all builtins are used in all configs")]
use brush_core::builtins::{self, builtin, decl_builtin, raw_arg_builtin, simple_builtin};

/// Returns the set of experimental built-in commands.
pub fn experimental_builtins<S: brush_core::ShellRuntime>()
-> std::collections::HashMap<String, builtins::Registration<S>> {
    let mut m = std::collections::HashMap::<String, builtins::Registration<S>>::new();

    #[cfg(feature = "builtin.save")]
    m.insert("save".into(), builtin::<save::SaveCommand, S>());

    m
}

/// Extension trait that simplifies adding experimental builtins to a shell builder.
pub trait ShellBuilderExt {
    /// Add experimental builtins to the shell being built.
    #[must_use]
    fn experimental_builtins(self) -> Self;
}

impl<S: brush_core::ShellBuilderState> ShellBuilderExt for brush_core::ShellBuilder<S> {
    fn experimental_builtins(self) -> Self {
        self.builtins(crate::experimental_builtins())
    }
}
