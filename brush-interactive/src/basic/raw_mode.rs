#[cfg(unix)]
use std::io::IsTerminal;

use crate::ShellError;

#[cfg(unix)]
pub(crate) struct RawModeToggle {
    initial: Option<nix::sys::termios::Termios>,
}

#[cfg(unix)]
impl RawModeToggle {
    pub fn new() -> Result<Self, ShellError> {
        if !std::io::stdin().is_terminal() {
            return Ok(Self { initial: None });
        }

        let initial = nix::sys::termios::tcgetattr(std::io::stdin())
            .map_err(|_err| ShellError::InputError)?;

        Ok(Self {
            initial: Some(initial),
        })
    }

    #[allow(clippy::unused_self)]
    pub fn enable(&self) -> Result<(), ShellError> {
        if !std::io::stdin().is_terminal() {
            return Ok(());
        }

        set_term_canonical(false)?;
        Ok(())
    }

    #[allow(clippy::unused_self)]
    pub fn disable(&self) -> Result<(), ShellError> {
        if !std::io::stdin().is_terminal() {
            return Ok(());
        }

        set_term_canonical(true)?;
        Ok(())
    }
}

#[cfg(unix)]
impl Drop for RawModeToggle {
    fn drop(&mut self) {
        if let Some(initial) = &self.initial {
            let _ = nix::sys::termios::tcsetattr(
                std::io::stdin(),
                nix::sys::termios::SetArg::TCSANOW,
                initial,
            );
        }
    }
}

#[cfg(unix)]
fn set_term_canonical(value: bool) -> Result<(), ShellError> {
    let mut termios =
        nix::sys::termios::tcgetattr(std::io::stdin()).map_err(|_err| ShellError::InputError)?;

    if value {
        termios.local_flags |= nix::sys::termios::LocalFlags::ICANON;
        termios.local_flags |= nix::sys::termios::LocalFlags::ISIG;
        termios.input_flags |= nix::sys::termios::InputFlags::IGNBRK;
        termios.input_flags |= nix::sys::termios::InputFlags::BRKINT;
    } else {
        termios.local_flags -= nix::sys::termios::LocalFlags::ICANON;
        termios.local_flags -= nix::sys::termios::LocalFlags::ISIG;
        termios.input_flags -= nix::sys::termios::InputFlags::IGNBRK;
        termios.input_flags -= nix::sys::termios::InputFlags::BRKINT;
    }

    nix::sys::termios::tcsetattr(
        std::io::stdin(),
        nix::sys::termios::SetArg::TCSANOW,
        &termios,
    )
    .map_err(|_err| ShellError::InputError)?;

    Ok(())
}

#[cfg(not(unix))]
pub(crate) struct RawModeToggle {
    initial: bool,
}

#[cfg(not(unix))]
impl RawModeToggle {
    pub fn new() -> Result<Self, ShellError> {
        let initial = crossterm::terminal::is_raw_mode_enabled()?;
        Ok(Self { initial })
    }

    #[allow(clippy::unused_self)]
    pub fn enable(&self) -> Result<(), ShellError> {
        crossterm::terminal::enable_raw_mode()?;
        Ok(())
    }

    #[allow(clippy::unused_self)]
    pub fn disable(&self) -> Result<(), ShellError> {
        crossterm::terminal::disable_raw_mode()?;
        Ok(())
    }
}

#[cfg(not(unix))]
impl Drop for RawModeToggle {
    fn drop(&mut self) {
        let _ = if self.initial {
            self.enable()
        } else {
            self.disable()
        };
    }
}
