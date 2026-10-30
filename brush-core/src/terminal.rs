//! Terminal control utilities.

use crate::{error, openfiles, sys};

/// Describes high-level terminal settings that can be requested.
#[derive(Default, bon::Builder)]
pub struct Settings {
    /// Whether to put the terminal in raw mode, as `cfmakeraw` does: input
    /// arrives a byte at a time, as typed, without being echoed, translated
    /// (e.g., Enter to newline), or turned into signals, and output isn't
    /// processed. The other settings then apply on top of it (e.g., to have
    /// newlines output as CRLF pairs).
    #[builder(default)]
    pub raw_input: bool,
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
