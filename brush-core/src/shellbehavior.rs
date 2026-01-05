//! Definition of shell behavior traits and defaults.

use crate::{ShellRuntime, error};

/// Trait for defining shell behavior.
pub trait ShellBehavior: Clone + Default + Send + Sync + 'static {
    /// Format the given error for display within the context of the provided shell.
    ///
    /// # Arguments
    ///
    /// * `error` - The error to format
    /// * `shell` - The shell context in which the error occurred.
    fn format_error(&self, error: &error::Error, shell: &impl ShellRuntime) -> String {
        let _ = shell;
        std::format!("error: {error:#}\n")
    }
}

/// Default shell behavior implementation.
#[derive(Clone, Default)]
pub struct DefaultShellBehavior;

impl ShellBehavior for DefaultShellBehavior {}
