//! Terminal control utilities.

use crate::{error, openfiles, sys};

/// Encapsulates the state of a controlled terminal.
pub struct TerminalControl {
    prev_fg_pid: Option<sys::process::ProcessId>,
}

impl TerminalControl {
    /// Acquire the terminal for the shell.
    pub fn acquire() -> Result<Self, error::Error> {
        let prev_fg_pid = sys::terminal::get_foreground_pid();

        // Break out into new process group.
        // TODO: jobs: Investigate why this sometimes fails with EPERM.
        let _ = sys::signal::lead_new_process_group();

        // Take ownership.
        sys::terminal::move_self_to_foreground()?;

        // Mask out SIGTTOU.
        sys::signal::mask_sigttou()?;

        Ok(Self { prev_fg_pid })
    }

    fn try_release(&mut self) {
        // Restore the previous foreground process group.
        if let Some(pid) = self.prev_fg_pid {
            if sys::terminal::move_to_foreground(pid).is_ok() {
                self.prev_fg_pid = None;
            }
        }
    }
}

impl Drop for TerminalControl {
    fn drop(&mut self) {
        self.try_release();
    }
}

/// Describes high-level terminal settings that can be requested.
#[derive(Default, bon::Builder)]
pub struct Settings {
    /// Whether to enable input echoing.
    pub echo_input: Option<bool>,
    /// Whether to enable line input (sometimes known as canonical mode).
    pub line_input: Option<bool>,
    /// Whether to disable interrupt signals and instead yield the control characters.
    pub interrupt_signals: Option<bool>,
    /// Whether to output newline characters as CRLF pairs.
    pub output_nl_as_nlcr: Option<bool>,
}

/// Guard that automatically restores terminal settings on drop.
pub struct AutoModeGuard {
    initial: sys::terminal::Config,
    file: openfiles::OpenFile,
}

impl AutoModeGuard {
    /// Creates a new `AutoModeGuard` for the given file.
    ///
    /// # Arguments
    ///
    /// * `file` - The file representing the terminal to control.
    pub fn new(file: openfiles::OpenFile) -> Result<Self, error::Error> {
        let initial = sys::terminal::Config::from_term(&file)?;
        Ok(Self { initial, file })
    }

    /// Applies the given terminal settings.
    ///
    /// # Arguments
    ///
    /// * `settings` - The terminal settings to apply.
    pub fn apply_settings(&self, settings: &Settings) -> Result<(), error::Error> {
        let mut config = sys::terminal::Config::from_term(&self.file)?;
        config.update(settings);
        config.apply_to_term(&self.file)?;

        Ok(())
    }
}

impl Drop for AutoModeGuard {
    fn drop(&mut self) {
        let _ = self.initial.apply_to_term(&self.file);
    }
}
