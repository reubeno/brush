use std::io::IsTerminal as _;
use std::io::Write as _;

use brush_core::Shell;

use crate::ShellError;

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

/// Extension trait providing interactive shell capabilities for a [`Shell`].
#[allow(async_fn_in_trait)]
pub trait InteractiveShellExt {
    /// Runs the interactive shell loop, reading commands from standard input and writing
    /// results to standard output and standard error. Continues until the shell
    /// normally exits or until a fatal error occurs.
    // NOTE: we use desugared async here because [async_fn_in_trait] "warning: use of `async fn` in
    // public traits is discouraged as auto trait bounds cannot be specified"
    async fn run_interactively(&self, input: &mut impl InputBackend) -> Result<(), ShellError>;

    /// Runs the interactive shell loop once, reading a single command from standard input.
    async fn run_interactively_once(
        &self,
        input: &mut impl InputBackend,
    ) -> Result<InteractiveExecutionResult, ShellError>;
}

impl InteractiveShellExt for crate::ShellRef {
    async fn run_interactively(&self, input: &mut impl InputBackend) -> Result<(), ShellError> {
        // Acquire terminal control if stdin is a terminal.
        if std::io::stdin().is_terminal() {
            brush_core::terminal::TerminalControl::acquire()?;
        }

        let mut shell = self.lock().await;

        let mut announce_exit = shell.options.interactive;

        shell.start_interactive_session()?;

        drop(shell);

        loop {
            let result = self.run_interactively_once(input).await?;
            match result {
                InteractiveExecutionResult::Executed(brush_core::ExecutionResult {
                    next_control_flow: brush_core::results::ExecutionControlFlow::ExitShell,
                    ..
                }) => {
                    break;
                }
                InteractiveExecutionResult::Executed(brush_core::ExecutionResult {
                    next_control_flow:
                        brush_core::results::ExecutionControlFlow::ReturnFromFunctionOrScript,
                    ..
                }) => {
                    tracing::error!("return from non-function/script");
                }
                InteractiveExecutionResult::Executed(_) => {}
                InteractiveExecutionResult::Failed(err) => {
                    // Report the error, but continue to execute.
                    let shell = self.lock().await;
                    let mut stderr = shell.stderr();
                    let _ = shell.display_error(&mut stderr, &err);

                    drop(shell);
                }
                InteractiveExecutionResult::Eof => {
                    break;
                }
            }

            if self.lock().await.options.exit_after_one_command {
                announce_exit = false;
                break;
            }
        }

        let mut shell = self.lock().await;

        shell.end_interactive_session()?;

        if announce_exit {
            writeln!(shell.stderr(), "exit")?;
        }

        if let Err(e) = shell.save_history() {
            // N.B. This seems like the sort of thing that's worth being noisy about,
            // but bash doesn't do that -- and probably for a reason.
            tracing::debug!("couldn't save history: {e}");
        }

        // Give the shell an opportunity to perform any on-exit operations.
        shell.on_exit().await?;

        drop(shell);

        Ok(())
    }

    async fn run_interactively_once(
        &self,
        input: &mut impl InputBackend,
    ) -> Result<InteractiveExecutionResult, ShellError> {
        let mut shell = self.lock().await;

        // Check for any completed jobs.
        shell.check_for_completed_jobs()?;

        // Run any pre-prompt commands.
        run_pre_prompt_commands(&mut shell).await?;

        // Now that we've done that, compose the prompt.
        let prompt = InteractivePrompt {
            prompt: shell.compose_prompt().await?,
            alt_side_prompt: shell.compose_alt_side_prompt().await?,
            continuation_prompt: shell.compose_continuation_prompt().await?,
        };

        drop(shell);

        match input.read_line(self, prompt)? {
            ReadResult::Input(read_result) => {
                let mut shell = self.lock().await;
                execute_line(&mut shell, input, read_result, true /* user input */).await
            }
            ReadResult::BoundCommand(read_result) => {
                let mut shell = self.lock().await;
                execute_line(&mut shell, input, read_result, false /* user input */).await
            }
            ReadResult::Eof => Ok(InteractiveExecutionResult::Eof),
            ReadResult::Interrupted => {
                let result: brush_core::ExecutionResult =
                    brush_core::ExecutionExitCode::Interrupted.into();
                self.lock()
                    .await
                    .set_last_exit_status(result.exit_code.into());
                Ok(InteractiveExecutionResult::Executed(result))
            }
        }
    }
}

/// Executes the given line of input.
async fn execute_line(
    shell: &mut Shell,
    input: &mut impl InputBackend,
    read_result: String,
    user_input: bool,
) -> Result<InteractiveExecutionResult, ShellError> {
    // See if the the user interface has a non-empty read buffer.
    let buffer_info = input.get_read_buffer();

    // If the user interface did, in fact, have a non-empty read buffer,
    // then reflect it to the shell in case any shell code wants to
    // process and/or transform the buffer.
    let nonempty_buffer = if let Some((buffer, cursor)) = buffer_info {
        if !buffer.is_empty() {
            shell.set_edit_buffer(buffer, cursor)?;
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
        let precmd_prompt = shell.compose_precmd_prompt().await?;
        if !precmd_prompt.is_empty() {
            print!("{precmd_prompt}");
        }

        // Update history (if applicable).
        shell.add_to_history(read_result.trim_end_matches('\n'))?;
    }

    // Count the command's lines.
    let line_count = read_result.lines().count().max(1);

    // Execute the command.
    let params = shell.default_exec_params();
    let source_info = brush_core::SourceInfo::from("main");
    let result = match shell.run_string(read_result, &source_info, &params).await {
        Ok(result) => Ok(InteractiveExecutionResult::Executed(result)),
        Err(e) => Ok(InteractiveExecutionResult::Failed(e)),
    };

    // Update cumulative line counter based on actual lines in the command.
    shell.increment_interactive_line_offset(line_count);

    // See if the shell has input buffer state that we need to reflect back to
    // the user interface. It may be state that originally came from the user
    // interface, or it may be state that was programmatically generated by
    // the command we just executed.
    let mut buffer_and_cursor = shell.pop_edit_buffer()?;

    if buffer_and_cursor.is_none() && nonempty_buffer {
        buffer_and_cursor = Some((String::new(), 0));
    }

    if let Some((updated_buffer, updated_cursor)) = buffer_and_cursor {
        input.set_read_buffer(updated_buffer, updated_cursor);
    }

    result
}

/// Represents an input backend for reading lines of input.
pub trait InputBackend: Send {
    /// Reads a line of input, using the given prompt.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell instance for which input is being read.
    /// * `prompt` - The prompt to display to the user.
    fn read_line(
        &mut self,
        shell: &crate::ShellRef,
        prompt: InteractivePrompt,
    ) -> Result<ReadResult, ShellError>;

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
}

async fn run_pre_prompt_commands(shell: &mut brush_core::Shell) -> Result<(), ShellError> {
    // If there's a variable called PROMPT_COMMAND, then run it first.
    if let Some(prompt_cmd_var) = shell.env_var("PROMPT_COMMAND") {
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
    let prev_last_result = shell.last_exit_status();
    let prev_last_pipeline_statuses = shell.last_pipeline_statuses.clone();

    // Run the command.
    let params = shell.default_exec_params();
    let source_info = brush_core::SourceInfo::from("PROMPT_COMMAND");
    shell.run_string(prompt_cmd, &source_info, &params).await?;

    // Restore the last exit status.
    shell.last_pipeline_statuses = prev_last_pipeline_statuses;
    shell.set_last_exit_status(prev_last_result);

    Ok(())
}
