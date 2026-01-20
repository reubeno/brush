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
        shell_ref: &crate::ShellRef<impl brush_core::ShellExtensions>,
        prompt: InteractivePrompt,
    ) -> Result<ReadResult, ShellError> {
        self.display_prompt(&prompt.prompt)?;

        let mut input = String::new();

        loop {
            let line = match Self::read_input_line()? {
                ReadResult::Input(s) => s,
                ReadResult::BoundCommand(s) => s,
                ReadResult::Eof => {
                    // If we have accumulated input, return it; otherwise EOF
                    if input.is_empty() {
                        return Ok(ReadResult::Eof);
                    }
                    break;
                }
                ReadResult::Interrupted => return Ok(ReadResult::Interrupted),
            };

            if line.is_empty() {
                if input.is_empty() {
                    return Ok(ReadResult::Eof);
                }
                break;
            }

            input.push_str(&line);

            // Check if the input is complete by trying to parse it
            if Self::is_input_complete(shell_ref, &input) {
                break;
            }

            // Input is incomplete, show continuation prompt and read more
            self.display_prompt(&prompt.continuation_prompt)?;
        }

        if input.is_empty() {
            Ok(ReadResult::Eof)
        } else {
            Ok(ReadResult::Input(input))
        }
    }
}

impl MinimalInputBackend {
    /// Check if the input is syntactically complete (not waiting for more input)
    fn is_input_complete(
        shell_ref: &crate::ShellRef<impl brush_core::ShellExtensions>,
        input: &str,
    ) -> bool {
        let shell = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(shell_ref.lock())
        });

        match shell.parse_string(input.to_owned()) {
            Err(brush_parser::ParseError::Tokenizing { inner, position: _ })
                if inner.is_incomplete() =>
            {
                false
            }
            Err(brush_parser::ParseError::ParsingAtEndOfInput) => false,
            _ => true,
        }
    }

    #[expect(clippy::unused_self)]
    fn should_display_prompt(&self) -> bool {
        std::io::stdin().is_terminal()
    }

    fn display_prompt(&self, prompt: &str) -> Result<(), ShellError> {
        if self.should_display_prompt() {
            eprint!("{prompt}");
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
