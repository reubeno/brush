use crate::error;
use std::os::fd::AsFd;

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
