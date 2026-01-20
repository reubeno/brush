use std::io::{BufRead, IsTerminal, Write};

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
        let interactive = std::io::stdin().is_terminal();
        self.read_line_from(
            shell_ref,
            &prompt,
            &mut std::io::stdin().lock(),
            interactive,
        )
    }
}

impl MinimalInputBackend {
    /// Core implementation that reads from any `BufRead` source.
    /// When `interactive` is true, prompts are displayed to stderr.
    #[doc(hidden)]
    pub fn read_line_from<R: BufRead>(
        &mut self,
        shell_ref: &crate::ShellRef<impl brush_core::ShellExtensions>,
        prompt: &InteractivePrompt,
        reader: &mut R,
        interactive: bool,
    ) -> Result<ReadResult, ShellError> {
        if interactive {
            Self::display_prompt(&prompt.prompt)?;
        }

        let mut input = String::new();

        loop {
            let line = match Self::read_input_line_from(reader)? {
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
            if interactive {
                Self::display_prompt(&prompt.continuation_prompt)?;
            }
        }

        if input.is_empty() {
            Ok(ReadResult::Eof)
        } else {
            Ok(ReadResult::Input(input))
        }
    }

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

    fn display_prompt(prompt: &str) -> Result<(), ShellError> {
        eprint!("{prompt}");
        std::io::stderr().flush()?;
        Ok(())
    }

    fn read_input_line_from<R: BufRead>(reader: &mut R) -> Result<ReadResult, ShellError> {
        let mut input = String::new();
        let bytes_read = reader
            .read_line(&mut input)
            .map_err(ShellError::InputError)?;

        if bytes_read == 0 {
            Ok(ReadResult::Eof)
        } else {
            Ok(ReadResult::Input(input))
        }
    }
}
