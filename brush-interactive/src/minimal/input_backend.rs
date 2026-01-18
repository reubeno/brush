use std::io::{IsTerminal, Write};

use crate::{
    InputBackend, ShellError,
    input_backend::{InteractivePrompt, ReadResult},
};

/// Represents a minimal shell input backend, capable of taking commands from standard input.
#[derive(Default)]
pub struct MinimalInputBackend;

impl InputBackend for MinimalInputBackend {
    fn read_line(
        &mut self,
        _shell_ref: &crate::ShellRef<impl brush_core::ShellExtensions>,
        prompt: InteractivePrompt,
    ) -> Result<ReadResult, ShellError> {
        self.display_prompt(&prompt)?;

        let result = match Self::read_input_line()? {
            ReadResult::Input(s) => s,
            ReadResult::BoundCommand(s) => s,
            ReadResult::Eof => return Ok(ReadResult::Eof),
            ReadResult::Interrupted => return Ok(ReadResult::Interrupted),
        };

        if result.is_empty() {
            Ok(ReadResult::Eof)
        } else {
            Ok(ReadResult::Input(result))
        }
    }
}

impl MinimalInputBackend {
    #[expect(clippy::unused_self)]
    fn should_display_prompt(&self) -> bool {
        std::io::stdin().is_terminal()
    }

    fn display_prompt(&self, prompt: &InteractivePrompt) -> Result<(), ShellError> {
        if self.should_display_prompt() {
            eprint!("{}", prompt.prompt);
            std::io::stderr().flush()?;
        }

        Ok(())
    }

    fn read_input_line() -> Result<ReadResult, ShellError> {
        let mut input = String::new();
        let bytes_read = std::io::stdin()
            .read_line(&mut input)
            .map_err(ShellError::InputError)?;

        if bytes_read == 0 {
            Ok(ReadResult::Eof)
        } else {
            Ok(ReadResult::Input(input))
        }
    }
}
