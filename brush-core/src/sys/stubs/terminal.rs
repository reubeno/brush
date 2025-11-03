//! Terminal utilities.

use crate::{error, sys, terminal};

/// Terminal configuration.
#[derive(Clone, Debug)]
pub struct Config;

#[allow(clippy::unused_self)]
impl Config {
    /// Creates a new `Config` from the actual terminal attributes of the terminal associated
    /// with the given file descriptor.
    ///
    /// # Arguments
    ///
    /// * `_fd` - The file descriptor of the terminal.
    pub fn from_term<Fd>(_fd: Fd) -> Result<Self, error::Error> {
        Ok(Self)
    }

    /// Applies the terminal settings to the terminal associated with the given file descriptor.
    ///
    /// # Arguments
    ///
    /// * `_fd` - The file descriptor of the terminal.
    pub fn apply_to_term<Fd>(&self, _fd: Fd) -> Result<(), error::Error> {
        Ok(())
    }

    /// Applies the given high-level terminal settings to this configuration. Does not modify any
    /// terminal itself.
    ///
    /// # Arguments
    ///
    /// * `_settings` - The high-level terminal settings to apply to this configuration.
    pub fn update(&mut self, _settings: &terminal::Settings) {}
}

/// Get the process ID of this process's parent.
///
/// This is a stub implementation that returns `None`.
pub fn get_parent_process_id() -> Option<sys::process::ProcessId> {
    None
}

/// Get the process group ID for this process's process group.
///
/// This is a stub implementation that returns `None`.
pub fn get_process_group_id() -> Option<sys::process::ProcessId> {
    None
}

/// Get the foreground process ID of the attached terminal.
///
/// This is a stub implementation that returns `None`.
pub fn get_foreground_pid() -> Option<sys::process::ProcessId> {
    None
}

/// Move the specified process to the foreground of the attached terminal.
///
/// This is a stub implementation that takes no action.
pub fn move_to_foreground(_pid: sys::process::ProcessId) -> Result<(), error::Error> {
    Ok(())
}

/// Moves the current process to the foreground of the attached terminal.
///
/// This is a stub implementation that returns `None`.
pub fn move_self_to_foreground() -> Result<(), error::Error> {
    Ok(())
}
