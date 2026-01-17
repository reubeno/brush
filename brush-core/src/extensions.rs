//! Definition of shell behavior traits and defaults.

use crate::{Shell, error, extensions};

/// Trait for static shell extensions. Collects all associated types needed to
/// instantiate a shell.
pub trait ShellExtensions: Clone + Default + Send + Sync + 'static {
    /// Type of the error behavior implementation.
    type ErrorBehavior: crate::ErrorBehavior;
}

#[derive(Clone, Default)]
pub struct ConstructedShellExtensions<EB: crate::ErrorBehavior> {
    _marker: std::marker::PhantomData<EB>,
}

impl<EB: crate::ErrorBehavior> ShellExtensions for ConstructedShellExtensions<EB> {
    type ErrorBehavior = EB;
}

/// Default shell extensions implementation.
#[derive(Clone, Default)]
pub struct DefaultShellExtensions;

impl ShellExtensions for DefaultShellExtensions {
    type ErrorBehavior = DefaultErrorBehavior;
}

/// Trait for defining shell error behaviors.
pub trait ErrorBehavior: Clone + Default + Send + Sync + 'static {
    /// Format the given error for display within the context of the provided shell.
    ///
    /// # Arguments
    ///
    /// * `error` - The error to format
    /// * `shell` - The shell context in which the error occurred.
    fn format_error(
        &self,
        error: &error::Error,
        shell: &Shell<impl extensions::ShellExtensions>,
    ) -> String {
        let _ = shell;
        std::format!("error: {error:#}\n")
    }
}

/// Default shell error behavior implementation.
#[derive(Clone, Default)]
pub struct DefaultErrorBehavior;

impl ErrorBehavior for DefaultErrorBehavior {}
