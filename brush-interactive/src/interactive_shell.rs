use crate::ShellError;
use std::io::{IsTerminal, Write};

/// Result of a read operation.
pub enum ReadResult {
    /// The user entered a line of input.
    Input(String),
    /// A bound key sequence yielded a registered command.
    BoundCommand(String),
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
pub trait InteractiveShell: Send {
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

    /// Returns the current contents of the read buffer and the current cursor
    /// position within the buffer; None is returned if the read buffer is
    /// empty or cannot be read by this implementation.
    fn get_read_buffer(&self) -> Option<(String, usize)> {
        None
    }

    /// Updates the read buffer with the given string and cursor. Considered a
    /// no-op if the implementation does not support updating read buffers.
    fn set_read_buffer(&mut self, _buffer: String, _cursor: usize) {
        // No-op by default.
    }

    /// Runs the interactive shell loop, reading commands from standard input and writing
    /// results to standard output and standard error. Continues until the shell
    /// normally exits or until a fatal error occurs.
    // NOTE: we use desugared async here because [async_fn_in_trait] "warning: use of `async fn` in
    // public traits is discouraged as auto trait bounds cannot be specified"
    fn run_interactively(
        &mut self,
    ) -> impl std::future::Future<Output = Result<(), ShellError>> + Send {
        async {
            // Acquire terminal control if stdin is a terminal.
            if std::io::stdin().is_terminal() {
                brush_core::TerminalControl::acquire()?;
            }

            let mut announce_exit = self.shell().as_ref().options.interactive;

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

                if self.shell().as_ref().options.exit_after_one_command {
                    announce_exit = false;
                    break;
                }
            }

            if announce_exit {
                writeln!(self.shell().as_ref().stderr(), "exit")?;
            }

            if let Err(e) = self.shell_mut().as_mut().save_history() {
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
    ) -> impl std::future::Future<Output = Result<InteractiveExecutionResult, ShellError>> + Send
    {
        async {
            let mut shell = self.shell_mut();
            let shell_mut = shell.as_mut();

            // Check for any completed jobs.
            shell_mut.check_for_completed_jobs()?;

            // Run any pre-prompt commands.
            run_pre_prompt_commands(shell_mut).await?;

            // Now that we've done that, compose the prompt.
            let prompt = InteractivePrompt {
                prompt: shell_mut.as_mut().compose_prompt().await?,
                alt_side_prompt: shell_mut.as_mut().compose_alt_side_prompt().await?,
                continuation_prompt: shell_mut.as_mut().compose_continuation_prompt().await?,
            };

            drop(shell);

            match self.read_line(prompt)? {
                ReadResult::Input(read_result) => {
                    self.execute_line(read_result, true /*user input*/).await
                }
                ReadResult::BoundCommand(read_result) => {
                    self.execute_line(read_result, false /*user input*/).await
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

    /// Executes the given line of input.
    fn execute_line(
        &mut self,
        read_result: String,
        user_input: bool,
    ) -> impl std::future::Future<Output = Result<InteractiveExecutionResult, ShellError>> + Send
    {
        async move {
            let buffer_info = self.get_read_buffer();

            let mut shell_mut = self.shell_mut();

            let nonempty_buffer = if let Some((buffer, cursor)) = buffer_info {
                if !buffer.is_empty() {
                    shell_mut.as_mut().set_edit_buffer(buffer, cursor)?;
                    true
                } else {
                    false
                }
            } else {
                false
            };

            // If the line came from direct user input (as opposed to a key binding, say), then we
            // need to do a few more things before executing it.
            if user_input {
                // Display the pre-command prompt (if there is one).
                let precmd_prompt = shell_mut.as_mut().compose_precmd_prompt().await?;
                if !precmd_prompt.is_empty() {
                    print!("{precmd_prompt}");
                }

                // Update history (if applicable).
                shell_mut
                    .as_mut()
                    .add_to_history(read_result.trim_end_matches('\n'))?;
            }

            // Execute the command.
            let params = shell_mut.as_mut().default_exec_params();
            let result = match shell_mut.as_mut().run_string(read_result, &params).await {
                Ok(result) => Ok(InteractiveExecutionResult::Executed(result)),
                Err(e) => Ok(InteractiveExecutionResult::Failed(e)),
            };

            if nonempty_buffer {
                let (updated_buffer, updated_cursor) = shell_mut
                    .as_mut()
                    .pop_edit_buffer()?
                    .unwrap_or((String::new(), 0));

                drop(shell_mut);

                self.set_read_buffer(updated_buffer, updated_cursor);
            }

            result
        }
    }
}

async fn run_pre_prompt_commands(shell: &mut brush_core::Shell) -> Result<(), ShellError> {
    // If there's a variable called PROMPT_COMMAND, then run it first.
    if let Some(prompt_cmd_var) = shell.get_env_var("PROMPT_COMMAND") {
        match prompt_cmd_var.value() {
            brush_core::ShellValue::String(cmd_str) => {
                run_pre_prompt_command(shell, cmd_str.to_owned()).await?;
            }
            brush_core::ShellValue::IndexedArray(values) => {
                let owned_values: Vec<_> = values.values().cloned().collect();
                for cmd_str in owned_values {
                    run_pre_prompt_command(shell, cmd_str).await?;
                }
            }
            // Other types are ignored.
            _ => (),
        }
    }

    Ok(())
}

async fn run_pre_prompt_command(
    shell: &mut brush_core::Shell,
    prompt_cmd: impl Into<String>,
) -> Result<(), ShellError> {
    // Save (and later restore) the last exit status.
    let prev_last_result = shell.last_exit_status;
    let prev_last_pipeline_statuses = shell.last_pipeline_statuses.clone();

    // Run the command.
    let params = shell.default_exec_params();
    shell.run_string(prompt_cmd, &params).await?;

    // Restore the last exit status.
    shell.last_pipeline_statuses = prev_last_pipeline_statuses;
    shell.last_exit_status = prev_last_result;

    Ok(())
}
