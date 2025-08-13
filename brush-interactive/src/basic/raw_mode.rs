use crate::ShellError;

pub(crate) struct RawModeToggle {
    initial: bool,
}

impl RawModeToggle {
    pub fn new() -> Result<Self, ShellError> {
        let initial = crossterm::terminal::is_raw_mode_enabled()?;
        Ok(Self { initial })
    }

    #[expect(clippy::unused_self)]
    pub fn enable(&self) -> Result<(), ShellError> {
        crossterm::terminal::enable_raw_mode()?;
        Ok(())
    }

    #[expect(clippy::unused_self)]
    pub fn disable(&self) -> Result<(), ShellError> {
        crossterm::terminal::disable_raw_mode()?;
        Ok(())
    }
}

impl Drop for RawModeToggle {
    fn drop(&mut self) {
        let _ = if self.initial {
            crossterm::terminal::enable_raw_mode()
        } else {
            crossterm::terminal::disable_raw_mode()
        };
    }
}
