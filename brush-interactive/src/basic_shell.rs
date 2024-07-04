use std::io::Write;

/// Represents a minimal shell capable of taking commands from standard input
/// and reporting results to standard output and standard error streams.
pub struct InteractiveShell {
    shell: brush_core::Shell,
}

impl InteractiveShell {
    /// Returns a new interactive shell instance, created with the provided options.
    ///
    /// # Arguments
    ///
    /// * `options` - Options for creating the interactive shell.
    pub async fn new(options: &crate::Options) -> Result<InteractiveShell, ShellError> {
        let shell = brush_core::Shell::new(&options.shell).await?;
        Ok(InteractiveShell { shell })
    }

    /// Returns an immutable reference to the inner shell object.
    pub fn shell(&self) -> &brush_core::Shell {
        &self.shell
    }

    /// Returns a mutable reference to the inner shell object.
    pub fn shell_mut(&mut self) -> &mut brush_core::Shell {
        &mut self.shell
    }

    /// Runs the interactive shell loop, reading commands from standard input and writing
    /// results to standard output and standard error. Continues until the shell
    /// normally exits or until a fatal error occurs.
    pub async fn run_interactively(&mut self) -> Result<(), ShellError> {
        loop {
            // Check for any completed jobs.
            self.shell_mut().check_for_completed_jobs()?;

            let result = self.run_interactively_once().await;
            match result {
                Ok(Some(brush_core::ExecutionResult {
                    exit_shell,
                    return_from_function_or_script,
                    ..
                })) => {
                    if exit_shell {
                        break;
                    }

                    if return_from_function_or_script {
                        tracing::error!("return from non-function/script");
                    }
                }
                Ok(None) => {
                    break;
                }
                Err(e) => {
                    // Report the error, but continue to execute.
                    tracing::error!("error: {:#}", e);
                }
            }
        }

        Ok(())
    }

    async fn run_interactively_once(
        &mut self,
    ) -> Result<Option<brush_core::ExecutionResult>, ShellError> {
        // Compose the prompt.
        let prompt = self.shell_mut().compose_prompt().await?;

        match self.readline(prompt.as_str()) {
            Some(read_result) => {
                let params = self.shell().default_exec_params();
                match self.shell_mut().run_string(read_result, &params).await {
                    Ok(result) => Ok(Some(result)),
                    Err(e) => Err(e.into()),
                }
            }
            None => Ok(None),
        }
    }

    fn readline(&mut self, prompt: &str) -> Option<String> {
        let _ = print!("{prompt}");
        let _ = std::io::stdout().flush();

        let mut buffer = String::new();
        let stdin = std::io::stdin(); // We get `Stdin` here.
        if let Ok(bytes_read) = stdin.read_line(&mut buffer) {
            if bytes_read > 0 {
                Some(buffer)
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Represents an error encountered while running or otherwise managing an interactive shell.
#[derive(thiserror::Error, Debug)]
pub enum ShellError {
    /// An error occurred with the embedded shell.
    #[error("{0}")]
    ShellError(#[from] brush_core::Error),

    /// A generic I/O error occurred.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}
