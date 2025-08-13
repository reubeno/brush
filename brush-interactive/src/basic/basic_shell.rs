use std::io::{IsTerminal, Write};

use crate::{
    ShellError, completion,
    interactive_shell::{InteractivePrompt, InteractiveShell, ReadResult},
};

use super::term_line_reader;

/// Represents a basic shell capable of interactive usage, with primitive support
/// for completion and test-focused automation via pexpect and similar technologies.
pub struct BasicShell {
    shell: brush_core::Shell,
}

impl BasicShell {
    /// Returns a new interactive shell instance, created with the provided options.
    ///
    /// # Arguments
    ///
    /// * `options` - Options for creating the interactive shell.
    pub async fn new(options: &crate::Options) -> Result<Self, ShellError> {
        let shell = brush_core::Shell::new(&options.shell).await?;
        Ok(Self { shell })
    }
}

impl InteractiveShell for BasicShell {
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
            match self.read_input_line(&prompt)? {
                ReadResult::Input(s) => {
                    result.push_str(s.as_str());
                    if self.is_valid_input(result.as_str()) {
                        break;
                    }
                }
                ReadResult::Eof => {
                    if result.is_empty() {
                        return Ok(ReadResult::Eof);
                    }
                    break;
                }
                ReadResult::Interrupted => return Ok(ReadResult::Interrupted),
            }
        }

        Ok(ReadResult::Input(result))
    }
}

impl BasicShell {
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

    fn read_input_line(&mut self, prompt: &InteractivePrompt) -> Result<ReadResult, ShellError> {
        if std::io::stdin().is_terminal() {
            term_line_reader::read_line(prompt.prompt.as_str(), |line, cursor| {
                self.generate_completions(line, cursor)
            })
        } else {
            let mut input = String::new();
            let bytes_read = std::io::stdin()
                .read_line(&mut input)
                .map_err(|_err| ShellError::InputError)?;

            if bytes_read == 0 {
                Ok(ReadResult::Eof)
            } else {
                Ok(ReadResult::Input(input))
            }
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

    fn generate_completions(
        &mut self,
        line: &str,
        cursor: usize,
    ) -> Result<brush_core::completion::Completions, ShellError> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(self.generate_completions_async(line, cursor))
        })
    }

    async fn generate_completions_async(
        &mut self,
        line: &str,
        cursor: usize,
    ) -> Result<brush_core::completion::Completions, ShellError> {
        Ok(completion::complete_async(&mut self.shell, line, cursor).await)
    }
}
