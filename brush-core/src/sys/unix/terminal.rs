use crate::{error, sys};
use std::{io::IsTerminal, os::fd::AsFd};

#[derive(Clone)]
pub(crate) struct TerminalSettings {
    termios: nix::sys::termios::Termios,
}

impl TerminalSettings {
    pub fn set_canonical(&mut self, value: bool) {
        self.set_local_flag(nix::sys::termios::LocalFlags::ICANON, value);
    }

    pub fn set_echo(&mut self, value: bool) {
        self.set_local_flag(nix::sys::termios::LocalFlags::ICANON, value);
    }

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

pub(crate) fn get_term_attr<Fd: AsFd>(fd: Fd) -> Result<TerminalSettings, error::Error> {
    Ok(TerminalSettings {
        termios: nix::sys::termios::tcgetattr(fd)?,
    })
}

pub(crate) fn set_term_attr_now<Fd: AsFd>(
    fd: Fd,
    settings: &TerminalSettings,
) -> Result<(), error::Error> {
    nix::sys::termios::tcsetattr(fd, nix::sys::termios::SetArg::TCSANOW, &settings.termios)?;
    Ok(())
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_parent_process_id() -> Option<sys::process::ProcessId> {
    Some(nix::unistd::getppid().as_raw())
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn get_process_group_id() -> Option<sys::process::ProcessId> {
    Some(nix::unistd::getpgrp().as_raw())
}

pub(crate) fn get_foreground_pid() -> Option<sys::process::ProcessId> {
    nix::unistd::tcgetpgrp(std::io::stdin())
        .ok()
        .map(|pgid| pgid.as_raw())
}

pub(crate) fn move_to_foreground(pid: sys::process::ProcessId) -> Result<(), error::Error> {
    nix::unistd::tcsetpgrp(std::io::stdin(), nix::unistd::Pid::from_raw(pid))?;
    Ok(())
}

pub(crate) fn move_self_to_foreground() -> Result<(), error::Error> {
    if std::io::stdin().is_terminal() {
        let pgid = nix::unistd::getpgid(None)?;

        // TODO: jobs: This sometimes fails with ENOTTY even though we checked that stdin is a
        // terminal. We should investigate why this is happening.
        let _ = nix::unistd::tcsetpgrp(std::io::stdin(), pgid);
    }

    Ok(())
}
