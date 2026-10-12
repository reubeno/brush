//! The terminal a [`super::VtInputBackend`] edits on.

use std::io::{IsTerminal, Read};
use std::ops::{Deref, DerefMut};
use std::time::Duration;

/// What a [`super::VtInputBackend`] needs from its terminal, beyond the shell's
/// standard error, where it draws.
///
/// Hosts that can't control the terminal through the platform (e.g., a browser
/// page running the shell as WebAssembly) provide their own implementation.
pub trait Terminal: Send {
    /// Switches the terminal to raw mode for line editing: input arrives a key at
    /// a time, without being echoed or interpreted. Output is still translated as
    /// usual, so a newline (`\n`) moves to the start of the next line.
    fn enable_raw_mode(&mut self) -> std::io::Result<()>;

    /// Restores the mode the terminal was in before [`Self::enable_raw_mode`].
    fn disable_raw_mode(&mut self) -> std::io::Result<()>;

    /// Returns the terminal's width, in columns, if known.
    fn columns(&self) -> Option<usize>;

    /// Whether there's a terminal to edit on. If not, lines are read as they come,
    /// without editing. By default, whether standard input is a terminal.
    fn is_terminal(&self) -> bool {
        std::io::stdin().is_terminal()
    }

    /// Reads what the terminal sends as keys are pressed. By default, from
    /// standard input.
    ///
    /// # Arguments
    ///
    /// * `buf` - Where to put what's read.
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        std::io::stdin().read(buf)
    }

    /// Waits up to `timeout` for the terminal to send more; returns whether it did.
    /// This tells a key that sends several bytes, which may arrive separately, from
    /// keys pressed one after another. By default, it doesn't wait, and reports that
    /// nothing came.
    ///
    /// # Arguments
    ///
    /// * `timeout` - How long to wait.
    fn wait_for_input(&mut self, timeout: Duration) -> std::io::Result<bool> {
        let _ = timeout;
        Ok(false)
    }
}

/// A [`Terminal`] in raw mode, from [`Self::enable`] until [`Self::restore`]. If
/// it's dropped first (e.g., when reading a line is cancelled), the terminal's
/// mode is restored then, and any error in restoring it is ignored.
pub(crate) struct RawMode<'a, T: Terminal> {
    terminal: &'a mut T,
    restored: bool,
}

impl<'a, T: Terminal> RawMode<'a, T> {
    /// Switches `terminal` to raw mode.
    ///
    /// # Arguments
    ///
    /// * `terminal` - The terminal to switch.
    pub fn enable(terminal: &'a mut T) -> std::io::Result<Self> {
        terminal.enable_raw_mode()?;
        Ok(Self {
            terminal,
            restored: false,
        })
    }

    /// Restores the mode the terminal was in before.
    pub fn restore(mut self) -> std::io::Result<()> {
        self.terminal.disable_raw_mode()?;
        self.restored = true;
        Ok(())
    }
}

impl<T: Terminal> Deref for RawMode<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.terminal
    }
}

impl<T: Terminal> DerefMut for RawMode<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.terminal
    }
}

impl<T: Terminal> Drop for RawMode<'_, T> {
    fn drop(&mut self) {
        if !self.restored {
            let _ = self.terminal.disable_raw_mode();
        }
    }
}

/// A [`Terminal`] for the process's own controlling terminal, controlled through
/// the platform's terminal APIs.
#[derive(Default)]
pub struct StdTerminal {
    mode: Option<brush_core::terminal::AutoModeGuard>,
}

impl Terminal for StdTerminal {
    fn enable_raw_mode(&mut self) -> std::io::Result<()> {
        // Restore any mode set before, so it isn't taken for the original one.
        self.mode = None;
        let mode = brush_core::terminal::AutoModeGuard::new(std::io::stdin().into())
            .map_err(std::io::Error::other)?;

        // Raw mode, as readline sets it, with Enter and Ctrl-J told apart, and a
        // read waiting for a byte; but with newlines output as CRLF pairs, as the
        // drawing expects.
        let settings = brush_core::terminal::Settings::builder()
            .raw_input(true)
            .output_nl_as_nlcr(true)
            .build();
        mode.apply_settings(&settings)
            .map_err(std::io::Error::other)?;

        self.mode = Some(mode);
        Ok(())
    }

    fn disable_raw_mode(&mut self) -> std::io::Result<()> {
        // Dropping the guard restores the original mode.
        self.mode = None;
        Ok(())
    }

    fn columns(&self) -> Option<usize> {
        std::env::var("COLUMNS").ok()?.parse().ok()
    }

    fn wait_for_input(&mut self, timeout: Duration) -> std::io::Result<bool> {
        match brush_core::sys::poll::poll_for_input(&std::io::stdin().into(), timeout) {
            // Where the platform can't wait for input, don't.
            Err(e) if e.kind() == std::io::ErrorKind::Unsupported => Ok(false),
            result => result,
        }
    }
}
