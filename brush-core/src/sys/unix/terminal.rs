//! Terminal utilities.

use crate::{error, sys, terminal};
use std::{io::IsTerminal, os::fd::AsFd};

/// Terminal configuration.
#[derive(Clone, Debug)]
pub struct Config {
    termios: nix::sys::termios::Termios,
}

impl Config {
    /// Creates a new `Config` from the actual terminal attributes of the terminal associated
    /// with the given file descriptor.
    ///
    /// # Arguments
    ///
    /// * `fd` - The file descriptor of the terminal.
    pub fn from_term(fd: impl AsFd) -> Result<Self, error::Error> {
        let termios = nix::sys::termios::tcgetattr(fd)?;
        Ok(Self { termios })
    }

    /// Applies the terminal settings to the terminal associated with the given file descriptor.
    ///
    /// # Arguments
    ///
    /// * `fd` - The file descriptor of the terminal.
    pub fn apply_to_term(&self, fd: impl AsFd) -> Result<(), error::Error> {
        nix::sys::termios::tcsetattr(fd, nix::sys::termios::SetArg::TCSANOW, &self.termios)?;
        Ok(())
    }

    /// Applies the given high-level terminal settings to this configuration. Does not modify any
    /// terminal itself.
    ///
    /// # Arguments
    ///
    /// * `settings` - The high-level terminal settings to apply to this configuration.
    pub fn update(&mut self, settings: &terminal::Settings) {
        if let Some(echo_input) = &settings.echo_input {
            if *echo_input {
                self.termios.local_flags |= nix::sys::termios::LocalFlags::ECHO;
            } else {
                self.termios.local_flags -= nix::sys::termios::LocalFlags::ECHO;
            }
        }

        if let Some(line_input) = &settings.line_input {
            if *line_input {
                self.termios.local_flags |= nix::sys::termios::LocalFlags::ICANON;
            } else {
                self.termios.local_flags -= nix::sys::termios::LocalFlags::ICANON;
            }
        }

        if let Some(interrupt_signals) = &settings.interrupt_signals {
            if *interrupt_signals {
                self.termios.local_flags |= nix::sys::termios::LocalFlags::ISIG;
            } else {
                self.termios.local_flags -= nix::sys::termios::LocalFlags::ISIG;
            }
        }

        if let Some(output_nl_as_nlcr) = &settings.output_nl_as_nlcr {
            if *output_nl_as_nlcr {
                self.termios.output_flags |=
                    nix::sys::termios::OutputFlags::OPOST | nix::sys::termios::OutputFlags::ONLCR;
            } else {
                self.termios.output_flags -= nix::sys::termios::OutputFlags::ONLCR;
            }
        }
    }
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
