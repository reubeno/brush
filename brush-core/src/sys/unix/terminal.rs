//! Terminal utilities.

use crate::{error, sys};
use std::{io::IsTerminal, os::fd::AsFd};

/// Terminal settings.
#[derive(Clone, Debug)]
pub struct TerminalSettings {
    termios: nix::sys::termios::Termios,
}

impl TerminalSettings {
    /// Sets canonical mode.
    ///
    /// # Arguments
    ///
    /// * `value` - If true, enable canonical mode; if false, disable it.
    pub fn set_canonical(&mut self, value: bool) {
        self.set_local_flag(nix::sys::termios::LocalFlags::ICANON, value);
    }

    /// Sets echo mode.
    ///
    /// # Arguments
    ///
    /// * `value` - If true, enable echo; if false, disable it.
    pub fn set_echo(&mut self, value: bool) {
        self.set_local_flag(nix::sys::termios::LocalFlags::ICANON, value);
    }

    /// Set interrupt signal mode.
    ///
    /// # Arguments
    ///
    /// * `value` - If true, enable interrupt signal; if false, disable it.
    pub fn set_int_signal(&mut self, value: bool) {
        self.set_local_flag(nix::sys::termios::LocalFlags::ISIG, value);
    }

    fn set_local_flag(&mut self, flag: nix::sys::termios::LocalFlags, value: bool) {
        if value {
            self.termios.local_flags.insert(flag);
        } else {
            self.termios.local_flags.remove(flag);
        }
    }
}

/// Gets the terminal attributes for the given file descriptor.
///
/// # Arguments
///
/// * `fd` - The file descriptor to get the terminal attributes for.
pub fn get_term_attr<Fd: AsFd>(fd: Fd) -> Result<TerminalSettings, error::Error> {
    Ok(TerminalSettings {
        termios: nix::sys::termios::tcgetattr(fd)?,
    })
}

/// Sets the terminal attributes for the given file descriptor immediately.
///
/// # Arguments
///
/// * `fd` - The file descriptor to set the terminal attributes for.
/// * `settings` - The terminal settings to apply.
pub fn set_term_attr_now<Fd: AsFd>(
    fd: Fd,
    settings: &TerminalSettings,
) -> Result<(), error::Error> {
    nix::sys::termios::tcsetattr(fd, nix::sys::termios::SetArg::TCSANOW, &settings.termios)?;
    Ok(())
}

/// Get the process ID of this process's parent.
pub fn get_parent_process_id() -> Option<sys::process::ProcessId> {
    Some(nix::unistd::getppid().as_raw())
}

/// Get the process group ID for this process's process group.
pub fn get_process_group_id() -> Option<sys::process::ProcessId> {
    Some(nix::unistd::getpgrp().as_raw())
}

/// Get the foreground process ID of the attached terminal.
pub fn get_foreground_pid() -> Option<sys::process::ProcessId> {
    nix::unistd::tcgetpgrp(std::io::stdin())
        .ok()
        .map(|pgid| pgid.as_raw())
}

/// Move the specified process to the foreground of the attached terminal.
pub fn move_to_foreground(pid: sys::process::ProcessId) -> Result<(), error::Error> {
    nix::unistd::tcsetpgrp(std::io::stdin(), nix::unistd::Pid::from_raw(pid))?;
    Ok(())
}

/// Moves the current process to the foreground of the attached terminal.
pub fn move_self_to_foreground() -> Result<(), error::Error> {
    if std::io::stdin().is_terminal() {
        let pgid = nix::unistd::getpgid(None)?;

        // TODO: jobs: This sometimes fails with ENOTTY even though we checked that stdin is a
        // terminal. We should investigate why this is happening.
        let _ = nix::unistd::tcsetpgrp(std::io::stdin(), pgid);
    }

    Ok(())
}
