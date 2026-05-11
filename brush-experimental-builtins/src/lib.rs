//! Experimental builtins.

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
