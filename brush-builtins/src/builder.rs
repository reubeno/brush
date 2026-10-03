use crate::BuiltinSet;

/// Extension trait that simplifies adding default builtins to a shell.
pub trait ShellExt {
    /// Register default builtins on the shell.
    ///
    /// # Arguments
    ///
    /// * `set` - The well-known set of built-ins to register.
    fn register_default_builtins(&mut self, set: BuiltinSet);
}

impl<SE: brush_core::extensions::ShellExtensions> ShellExt for brush_core::Shell<SE> {
    fn register_default_builtins(&mut self, set: BuiltinSet) {
        crate::register_default_builtins(self, set);
    }
}
