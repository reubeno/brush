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
        let mut result = String::new();
        let mut prompt_to_display = Some(&prompt);

        loop {
            self.display_prompt_for_continuation(prompt_to_display)?;

            let line = match Self::read_input_line()? {
                ReadResult::Input(s) => s,
                ReadResult::BoundCommand(s) => s,
                ReadResult::Eof => {
                    if result.is_empty() {
                        return Ok(ReadResult::Eof);
                    }
                    break;
                }
                ReadResult::Interrupted => return Ok(ReadResult::Interrupted),
            };

            result.push_str(&line);

            if Self::is_complete_input(shell_ref, &result) {
                break;
            }

            prompt_to_display = None;
        }

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

    fn display_prompt_for_continuation(
        &self,
        prompt: Option<&InteractivePrompt>,
    ) -> Result<(), ShellError> {
        if self.should_display_prompt() {
            if let Some(p) = prompt {
                eprint!("{}", p.prompt);
            } else {
                eprint!("> ");
            }
            std::io::stderr().flush()?;
        }

        Ok(())
    }

    fn is_complete_input(
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
