use std::io::{IsTerminal, Write};

use crate::{
    ShellError,
    interactive_shell::{InteractivePrompt, InteractiveShell, ReadResult},
};

/// Represents a minimal shell capable of taking commands from standard input
/// and reporting results to standard output and standard error streams.
pub struct MinimalShell {
    shell: brush_core::Shell,
}

impl MinimalShell {
    /// Returns a new interactive shell instance, created with the provided options.
    ///
    /// # Arguments
    ///
    /// * `options` - Options for creating the interactive shell.
    pub async fn new(options: crate::Options) -> Result<Self, ShellError> {
        let shell = brush_core::Shell::new(options.shell).await?;
        Ok(Self { shell })
    }
}

impl InteractiveShell for MinimalShell {
    /// Returns an immutable reference to the inner shell object.
    fn shell(&self) -> impl AsRef<brush_core::Shell> {
        self.shell.as_ref()
    }

    /// Returns a mutable reference to the inner shell object.
    fn shell_mut(&mut self) -> impl AsMut<brush_core::Shell> {
        self.shell.as_mut()
    }

    fn read_line(&mut self, prompt: InteractivePrompt) -> Result<ReadResult, ShellError> {
        self.display_prompt(&prompt)?;

        let mut result = String::new();

        loop {
            match Self::read_input_line()? {
                ReadResult::Input(s) => {
                    result.push_str(s.as_str());
                    if self.is_valid_input(result.as_str()) {
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

impl MinimalShell {
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

    fn is_valid_input(&self, input: &str) -> bool {
        match self.shell.parse_string(input.to_owned()) {
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
