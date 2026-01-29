//! Definition of shell behavior traits and defaults.

use crate::filter::{CmdExecFilter, NoOpCmdExecFilter, NoOpSourceFilter, SourceFilter};
use crate::{Shell, error, extensions};

/// Trait for static shell extensions. Collects all associated types needed to
/// instantiate a shell into a single containing struct.
pub trait ShellExtensions: Clone + Default + Send + Sync + 'static {
    /// Type of the error behavior implementation.
    type ErrorFormatter: ErrorFormatter;
    /// Type of the command execution filter.
    type CmdExecFilter: CmdExecFilter;
    /// Type of the source filter.
    type SourceFilter: SourceFilter;
}

/// Shell extensions implementation constructed from component types.
#[derive(Clone, Default)]
pub struct ShellExtensionsImpl<
    EF: ErrorFormatter = DefaultErrorFormatter,
    CF: CmdExecFilter = NoOpCmdExecFilter,
    SF: SourceFilter = NoOpSourceFilter,
> {
    error_formatter: EF,
    cmd_exec_filter: CF,
    source_filter: SF,
}

impl<EF: ErrorFormatter, CF: CmdExecFilter, SF: SourceFilter> ShellExtensions
    for ShellExtensionsImpl<EF, CF, SF>
{
    type ErrorFormatter = EF;
    type CmdExecFilter = CF;
    type SourceFilter = SF;
}

impl<EF: ErrorFormatter, CF: CmdExecFilter, SF: SourceFilter> ShellExtensionsImpl<EF, CF, SF> {
    /// Returns a reference to the error formatter.
    pub const fn error_formatter(&self) -> &EF {
        &self.error_formatter
    }

    /// Returns a reference to the command execution filter.
    pub const fn cmd_exec_filter(&self) -> &CF {
        &self.cmd_exec_filter
    }

    /// Returns a reference to the source filter.
    pub const fn source_filter(&self) -> &SF {
        &self.source_filter
    }
}

/// Default shell extensions implementation.
/// This is a type alias for the most common shell configuration.
pub type DefaultShellExtensions = ShellExtensionsImpl<DefaultErrorFormatter>;

/// Trait for defining shell error behaviors.
pub trait ErrorFormatter: Clone + Default + Send + Sync + 'static {
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
pub struct DefaultErrorFormatter;

impl ErrorFormatter for DefaultErrorFormatter {}

/// Trait for placeholder behavior (stub for future extension).
pub trait PlaceholderBehavior: Clone + Default + Send + Sync + 'static {}

/// Default placeholder implementation.
#[derive(Clone, Default)]
pub struct DefaultPlaceholder;

impl PlaceholderBehavior for DefaultPlaceholder {}
