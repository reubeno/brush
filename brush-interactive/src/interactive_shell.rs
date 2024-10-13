use crate::ShellError;
use std::io::Write;

/// Result of a read operation.
pub enum ReadResult {
    /// The user entered a line of input.
    Input(String),
    /// End of input was reached.
    Eof,
    /// The user interrupted the input operation.
    Interrupted,
}

/// Result of an interactive execution.
pub enum InteractiveExecutionResult {
    /// The command was executed and returned the given result.
    Executed(brush_core::ExecutionResult),
    /// The command failed to execute.
    Failed(brush_core::Error),
    /// End of input was reached.
    Eof,
}

/// Represents an interactive prompt.
pub struct InteractivePrompt {
    /// Prompt to display.
    pub prompt: String,
    /// Alternate-side prompt (typically right) to display.
    pub alt_side_prompt: String,
    /// Prompt to display on a continuation line of input.
    pub continuation_prompt: String,
}

/// Represents a shell capable of taking commands from standard input.

pub trait InteractiveShell {
    /// Returns an immutable reference to the inner shell object.
    fn shell(&self) -> impl AsRef<brush_core::Shell> + Send;

    /// Returns a mutable reference to the inner shell object.
    fn shell_mut(&mut self) -> impl AsMut<brush_core::Shell> + Send;

    /// Reads a line of input, using the given prompt.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The prompt to display to the user.
    fn read_line(&mut self, prompt: InteractivePrompt) -> Result<ReadResult, ShellError>;

    /// Update history, if relevant.
    fn update_history(&mut self) -> Result<(), ShellError>;

    /// Runs the interactive shell loop, reading commands from standard input and writing
    /// results to standard output and standard error. Continues until the shell
    /// normally exits or until a fatal error occurs.
    // NOTE: we use desugared async here because [async_fn_in_trait] "warning: use of `async fn` in public traits is discouraged as auto trait bounds cannot be specified"
    fn run_interactively(&mut self) -> impl std::future::Future<Output = Result<(), ShellError>> {
        async {
            // TODO: Consider finding a better place for this.
            let _ = brush_core::TerminalControl::acquire()?;

            loop {
                let result = self.run_interactively_once().await?;
                match result {
                    InteractiveExecutionResult::Executed(brush_core::ExecutionResult {
                        exit_shell,
                        return_from_function_or_script,
                        ..
                    }) => {
                        if exit_shell {
                            break;
                        }

                        if return_from_function_or_script {
                            tracing::error!("return from non-function/script");
                        }
                    }
                    InteractiveExecutionResult::Failed(e) => {
                        // Report the error, but continue to execute.
                        tracing::error!("error: {:#}", e);
                    }
                    InteractiveExecutionResult::Eof => {
                        break;
                    }
                }
            }

            if self.shell().as_ref().options.interactive {
                writeln!(self.shell().as_ref().stderr(), "exit")?;
            }

            if let Err(e) = self.update_history() {
                // N.B. This seems like the sort of thing that's worth being noisy about,
                // but bash doesn't do that -- and probably for a reason.
                tracing::debug!("couldn't save history: {e}");
            }

            Ok(())
        }
    }

    /// Runs the interactive shell loop once, reading a single command from standard input.
    fn run_interactively_once(
        &mut self,
    ) -> impl std::future::Future<Output = Result<InteractiveExecutionResult, ShellError>> {
        async {
            let mut shell_mut = self.shell_mut();

            // Check for any completed jobs.
            shell_mut.as_mut().check_for_completed_jobs()?;

            // If there's a variable called PROMPT_COMMAND, then run it first.
            if let Some((_, prompt_cmd)) = shell_mut.as_mut().env.get("PROMPT_COMMAND") {
                let prompt_cmd = prompt_cmd.value().to_cow_string().to_string();

                // Save (and later restore) the last exit status.
                let prev_last_result = shell_mut.as_mut().last_exit_status;

                let params = shell_mut.as_mut().default_exec_params();

                shell_mut.as_mut().run_string(prompt_cmd, &params).await?;
                shell_mut.as_mut().last_exit_status = prev_last_result;
            }

            // Now that we've done that, compose the prompt.
            let prompt = InteractivePrompt {
                prompt: shell_mut.as_mut().compose_prompt().await?,
                alt_side_prompt: shell_mut.as_mut().compose_alt_side_prompt().await?,
                continuation_prompt: shell_mut.as_mut().continuation_prompt()?,
            };

            drop(shell_mut);

            match self.read_line(prompt)? {
                ReadResult::Input(read_result) => {
                    let mut shell_mut = self.shell_mut();
                    let params = shell_mut.as_mut().default_exec_params();
                    match shell_mut.as_mut().run_string(read_result, &params).await {
                        Ok(result) => Ok(InteractiveExecutionResult::Executed(result)),
                        Err(e) => Ok(InteractiveExecutionResult::Failed(e)),
                    }
                }
                ReadResult::Eof => Ok(InteractiveExecutionResult::Eof),
                ReadResult::Interrupted => {
                    let mut shell_mut = self.shell_mut();
                    shell_mut.as_mut().last_exit_status = 130;
                    Ok(InteractiveExecutionResult::Executed(
                        brush_core::ExecutionResult::new(130),
                    ))
                }
            }
        }
    }
}
