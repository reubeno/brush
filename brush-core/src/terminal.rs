use crate::{error, sys};

/// Encapsulates the state of a controlled terminal.
#[expect(clippy::module_name_repetitions)]
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
