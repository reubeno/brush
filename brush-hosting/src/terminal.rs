//! The terminal, and the process group that has it, for an interactive shell's job control.
//!
//! Under job control, an interactive shell runs each job in a process group of its own, and
//! gives the terminal to the one in the foreground. For that, the shell's process leads a
//! process group of its own, and has the terminal while it reads commands.
//! [`ProcessTerminal::acquire`] takes both, and gives the terminal back once what it
//! returns is dropped.

mod sys;

use std::io::IsTerminal as _;

use brush_core::sys::{process::ProcessId, terminal};

/// The terminal on the process's standard input, taken over by the process for its shells'
/// job control (see the module). Dropping it gives the terminal back to the process group
/// that had it.
#[derive(Debug)]
pub struct ProcessTerminal {
    prev_fg_pid: Option<ProcessId>,
}

impl ProcessTerminal {
    /// Takes the terminal for the process, if its standard input is one: has the process ignore
    /// SIGTTOU, moves it into a process group of its own, and makes that group the terminal's
    /// foreground. Without a terminal, there's nothing to take, and this changes nothing.
    ///
    /// # Errors
    ///
    /// Fails if the process can't ignore SIGTTOU, or can't take the terminal's foreground.
    pub fn acquire() -> std::io::Result<Self> {
        if !std::io::stdin().is_terminal() {
            return Ok(Self { prev_fg_pid: None });
        }

        // First: leading a group of its own, the process takes the foreground from the
        // background, which the kernel answers with SIGTTOU, whose default action would stop
        // it. The programs the shell starts inherit the ignore.
        sys::ignore_sigttou()?;

        let prev_fg_pid = terminal::get_foreground_pid();

        // TODO(jobs): Investigate why this sometimes fails with EPERM.
        let _ = sys::lead_new_process_group();

        terminal::move_self_to_foreground()?;

        Ok(Self { prev_fg_pid })
    }
}

impl Drop for ProcessTerminal {
    fn drop(&mut self) {
        if let Some(pid) = self.prev_fg_pid {
            let _ = terminal::move_to_foreground(pid);
        }
    }
}
