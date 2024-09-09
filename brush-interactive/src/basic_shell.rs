use std::io::Write;

use crate::{
    interactive_shell::{InteractiveShell, ReadResult},
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

    fn read_line(&mut self, prompt: &str) -> Result<ReadResult, ShellError> {
        print!("{prompt}");
        let _ = std::io::stdout().flush();

        let mut buffer = String::new();
        let stdin = std::io::stdin(); // We get `Stdin` here.
        if let Ok(bytes_read) = stdin.read_line(&mut buffer) {
            if bytes_read > 0 {
                Ok(ReadResult::Input(buffer))
            } else {
                Ok(ReadResult::Eof)
            }
        } else {
            Err(ShellError::InputError)
        }
    }

    fn update_history(&mut self) -> Result<(), ShellError> {
        Ok(())
    }
}
