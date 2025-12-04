use std::io::{IsTerminal, Write};

use brush_core::Shell;

use crate::{
    InputBackend, ShellError,
    interactive_shell::{InteractivePrompt, ReadResult},
};

/// Represents a minimal shell input backend, capable of taking commands from standard input.
#[derive(Default)]
pub struct MinimalInputBackend;

impl InputBackend for MinimalInputBackend {
    fn read_line(
        &mut self,
        shell_ref: &crate::ShellRef,
        prompt: InteractivePrompt,
    ) -> Result<ReadResult, ShellError> {
        self.display_prompt(&prompt)?;

        let mut result = String::new();

        loop {
            match Self::read_input_line()? {
                ReadResult::Input(s) => {
                    result.push_str(s.as_str());

                    let shell = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(shell_ref.lock())
                    });

                    if Self::is_valid_input(&shell, result.as_str()) {
                        break;
                    }
                }
                ReadResult::BoundCommand(s) => {
                    result.push_str(s.as_str());
                    break;
                }
                ReadResult::Eof => break,
                ReadResult::Interrupted => return Ok(ReadResult::Interrupted),
            }
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

    fn is_valid_input(shell: &Shell, input: &str) -> bool {
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
}
