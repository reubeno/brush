use std::io::{IsTerminal, Write};

use crate::{
    interactive_shell::{InteractivePrompt, InteractiveShell, ReadResult},
    ShellError,
};

/// Represents a minimal shell capable of taking commands from standard input
/// and reporting results to standard output and standard error streams.
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
        if self.should_display_prompt() {
            print!("{}", prompt.prompt);
            let _ = std::io::stdout().flush();
        }

        let stdin = std::io::stdin();
        let mut result = String::new();

        while result.is_empty() || !self.is_valid_input(result.as_str()) {
            let mut read_buffer = String::new();
            let bytes_read = stdin
                .read_line(&mut read_buffer)
                .map_err(|_err| ShellError::InputError)?;

            if bytes_read == 0 {
                break;
            }

            result.push_str(read_buffer.as_str());
        }

        if result.is_empty() {
            Ok(ReadResult::Eof)
        } else {
            Ok(ReadResult::Input(result))
        }
    }

    fn update_history(&mut self) -> Result<(), ShellError> {
        Ok(())
    }
}

impl BasicShell {
    #[allow(clippy::unused_self)]
    fn should_display_prompt(&self) -> bool {
        std::io::stdin().is_terminal()
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
