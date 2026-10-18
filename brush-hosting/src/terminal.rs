//! The terminal, and the process group that has it, for an interactive shell's job control.
//!
//! Under job control, an interactive shell runs each job in a process group of its own, and
//! gives the terminal to the one in the foreground. For that, the shell's process leads a
//! process group of its own, and has the terminal while it reads commands.
//! [`ControllingTerminal::acquire`] takes both, and gives the terminal back once what it
//! returns is dropped. Named as a shell's [`Terminal`](brush_core::extensions::Terminal),
//! [`ControllingTerminal`] lets the shell move the foreground between itself and its jobs.

mod sys;

use std::io::IsTerminal as _;
use std::sync::Arc;

use brush_core::sys::{process::ProcessId, terminal};

/// The process's controlling terminal, for a shell's job control (see the module).
///
/// Any value moves the foreground, for the shell that names this as its `Terminal`; the one
/// [`Self::acquire`] returns also holds what was taken, and gives it back when it, and every
/// copy of it, is dropped.
#[derive(Clone, Debug, Default)]
pub struct ControllingTerminal {
    /// Held for its drop.
    _taken: Option<Arc<Taken>>,
}

impl ControllingTerminal {
    /// Takes the terminal for the process, if its standard input is one: has the process ignore
    /// SIGTTOU, moves it into a process group of its own, and makes that group the terminal's
    /// foreground. Dropping the result, and every copy of it, gives the terminal back to the
    /// group that had it. Without a terminal, there's nothing to take, and this changes nothing.
    ///
    /// # Errors
    ///
    /// Fails if the process can't ignore SIGTTOU, or can't take the terminal's foreground.
    pub fn acquire() -> std::io::Result<Self> {
        if !std::io::stdin().is_terminal() {
            return Ok(Self::default());
        }

        // First: leading a group of its own, the process takes the foreground from the
        // background, which the kernel answers with SIGTTOU, whose default action would stop
        // it. The programs the shell starts inherit the ignore.
        sys::ignore_sigttou()?;

        let prev_fg_pid = terminal::get_foreground_pid();

        // TODO(jobs): Investigate why this sometimes fails with EPERM.
        let _ = sys::lead_new_process_group();

        terminal::move_self_to_foreground()?;

        Ok(Self {
            _taken: Some(Arc::new(Taken { prev_fg_pid })),
        })
    }
}

impl brush_core::extensions::Terminal for ControllingTerminal {
    fn give_foreground(&self, group: ProcessId) -> std::io::Result<()> {
        terminal::move_to_foreground(group)
    }

    fn take_foreground(&self) -> std::io::Result<()> {
        terminal::move_self_to_foreground()
    }
}

/// The terminal's foreground, taken from the process group that had it, which gets it back on drop.
#[derive(Debug)]
struct Taken {
    prev_fg_pid: Option<ProcessId>,
}

impl Drop for Taken {
    fn drop(&mut self) {
        if let Some(pid) = self.prev_fg_pid {
            let _ = terminal::move_to_foreground(pid);
        }
    }
}
