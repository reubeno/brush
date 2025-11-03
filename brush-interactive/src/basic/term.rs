use crate::ShellError;

pub(crate) struct TerminalMode {
    initial_attr: brush_core::sys::terminal::TerminalSettings,
}

impl TerminalMode {
    pub fn new() -> Result<Self, ShellError> {
        let initial_attr = brush_core::sys::terminal::get_term_attr(std::io::stdin())?;
        Ok(Self { initial_attr })
    }

    #[allow(dead_code)]
    pub fn enable_canonical_mode(&self) -> Result<(), ShellError> {
        #[cfg(unix)]
        self.update_termios(|termios| {
            termios.local_flags |= nix::sys::termios::LocalFlags::ICANON;
        })?;
        Ok(())
    }

    pub fn disable_canonical_mode(&self) -> Result<(), ShellError> {
        #[cfg(unix)]
        self.update_termios(|termios| {
            termios.local_flags -= nix::sys::termios::LocalFlags::ICANON;
        })?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn enable_int_signal(&self) -> Result<(), ShellError> {
        #[cfg(unix)]
        self.update_termios(|termios| {
            termios.local_flags |= nix::sys::termios::LocalFlags::ISIG;
        })?;
        Ok(())
    }

    pub fn disable_int_signal(&self) -> Result<(), ShellError> {
        #[cfg(unix)]
        self.update_termios(|termios| {
            termios.local_flags -= nix::sys::termios::LocalFlags::ISIG;
        })?;
        Ok(())
    }

    pub fn enable_output_processing(&self) -> Result<(), ShellError> {
        #[cfg(unix)]
        self.update_termios(|termios| {
            termios.output_flags |= nix::sys::termios::OutputFlags::OPOST;
        })?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn disable_output_processing(&self) -> Result<(), ShellError> {
        #[cfg(unix)]
        self.update_termios(|termios| {
            termios.output_flags -= nix::sys::termios::OutputFlags::OPOST;
        })?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn enable_echo(&self) -> Result<(), ShellError> {
        #[cfg(unix)]
        self.update_termios(|termios| {
            termios.local_flags |= nix::sys::termios::LocalFlags::ECHO;
        })?;
        Ok(())
    }

    pub fn disable_echo(&self) -> Result<(), ShellError> {
        #[cfg(unix)]
        self.update_termios(|termios| {
            termios.local_flags -= nix::sys::termios::LocalFlags::ECHO;
        })?;
        Ok(())
    }

    pub fn enable_nlcr(&self) -> Result<(), ShellError> {
        #[cfg(unix)]
        self.update_termios(|termios| {
            termios.output_flags |= nix::sys::termios::OutputFlags::ONLCR;
        })?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn disable_nlcr(&self) -> Result<(), ShellError> {
        #[cfg(unix)]
        self.update_termios(|termios| {
            termios.output_flags -= nix::sys::termios::OutputFlags::ONLCR;
        })?;
        Ok(())
    }

    #[expect(clippy::unused_self)]
    #[cfg(unix)]
    fn update_termios(
        &self,
        updater: impl Fn(&mut nix::sys::termios::Termios),
    ) -> Result<(), ShellError> {
        let mut termios = nix::sys::termios::tcgetattr(std::io::stdin())
            .map_err(|_err| ShellError::InputError)?;

        updater(&mut termios);

        nix::sys::termios::tcsetattr(
            std::io::stdin(),
            nix::sys::termios::SetArg::TCSANOW,
            &termios,
        )
        .map_err(|_err| ShellError::InputError)?;

        Ok(())
    }
}

impl Drop for TerminalMode {
    fn drop(&mut self) {
        let _ = brush_core::sys::terminal::set_term_attr_now(std::io::stdin(), &self.initial_attr);
    }
}
