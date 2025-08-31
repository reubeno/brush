//! Terminal utilities.

use crate::{error, sys};

/// Terminal settings.
#[derive(Clone)]
pub struct TerminalSettings {}

impl TerminalSettings {
    /// Sets canonical mode.
    /// Sets canonical mode.
    ///
    /// This is a stub implementation that does nothing.
    pub fn set_canonical(&mut self, _value: bool) {}

    /// Sets echo mode.
    ///
    /// This is a stub implementation that does nothing.
    pub fn set_echo(&mut self, _value: bool) {}

    /// Sets interrupt signal mode.
    ///
    /// This is a stub implementation that does nothing.
    pub fn set_int_signal(&mut self, _value: bool) {}
}

/// Gets the terminal attributes for the given file descriptor.
///
/// This is a stub implementation that returns default settings.
pub fn get_term_attr<Fd>(_fd: Fd) -> Result<TerminalSettings, error::Error> {
    Ok(TerminalSettings {})
}

/// Sets the terminal attributes for the given file descriptor immediately.
///
/// This is a stub implementation that does nothing.
pub fn set_term_attr_now<Fd>(_fd: Fd, _settings: &TerminalSettings) -> Result<(), error::Error> {
    Ok(())
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
