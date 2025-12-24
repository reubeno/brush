use std::io::IsTerminal as _;
use std::io::Write as _;

use crate::InputBackend;
use crate::InteractivePrompt;
use crate::ReadResult;
use crate::ShellError;
use crate::UIOptions;

/// Result of an interactive execution.
pub enum InteractiveExecutionResult {
    /// The command was executed and returned the given result.
    Executed(brush_core::ExecutionResult),
    /// The command failed to execute.
    Failed(brush_core::Error),
    /// End of input was reached.
    Eof,
}

impl From<&InteractiveExecutionResult> for i32 {
    /// Converts an `InteractiveExecutionResult` into a signed, 32-bit exit code.
    fn from(value: &InteractiveExecutionResult) -> Self {
        match value {
            InteractiveExecutionResult::Executed(result) => u8::from(result.exit_code).into(),
            InteractiveExecutionResult::Failed(_) => 1,
            InteractiveExecutionResult::Eof => 0,
        }
    }
}

/// Represents an interactive shell that displays prompts, interactively reads user input, etc.
pub struct InteractiveShell<'a, IB: InputBackend> {
    /// The underlying shell instance.
    shell: crate::ShellRef,
    /// The input backend to use.
    input: &'a mut IB,
    /// Terminal integration utility, if any.
    terminal_integration: Option<crate::term_integration::TerminalIntegration>,
}

impl<'a, IB: InputBackend> InteractiveShell<'a, IB> {
    /// Creates a new `InteractiveShell` wrapping the given shell instance.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell instance to wrap.
    /// * `input` - The input backend to use.
    /// * `options` - The user interface options to use.
    pub fn new(
        shell: &crate::ShellRef,
        input: &'a mut IB,
        options: &UIOptions,
    ) -> Result<Self, ShellError> {
        // Acquire terminal control if stdin is a terminal.
        if std::io::stdin().is_terminal() {
            brush_core::terminal::TerminalControl::acquire()?;
        }

        // Set up terminal integration if enabled.
        let terminal_integration = if options.terminal_shell_integration {
            let terminal_integration = crate::term_integration::TerminalIntegration::new(
                crate::term::get_terminal_info(&HostEnvironment),
            );

            print!("{}", terminal_integration.initialize().as_ref());
            let _ = std::io::stdout().flush();

            Some(terminal_integration)
        } else {
            None
        };

        Ok(Self {
            shell: shell.clone(),
            input,
            terminal_integration,
        })
    }

    /// Runs the interactive shell loop, reading commands from standard input and writing
    /// results to standard output and standard error. Continues until the shell
    /// normally exits or until a fatal error occurs.
    pub async fn run_interactively(&mut self) -> Result<(), ShellError> {
        let mut shell = self.shell.lock().await;

        let mut announce_exit = shell.options.interactive;

        shell.start_interactive_session()?;

        drop(shell);

        loop {
            let result = self.run_interactively_once().await?;
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
                    let shell = self.shell.lock().await;
                    let mut stderr = shell.stderr();
                    let _ = shell.display_error(&mut stderr, &err).await;

                    drop(shell);
                }
                InteractiveExecutionResult::Eof => {
                    break;
                }
            }

            if self.shell.lock().await.options.exit_after_one_command {
                announce_exit = false;
                break;
            }
        }

        let mut shell = self.shell.lock().await;

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

    /// Runs the interactive shell loop once, reading a single command from standard input.
    async fn run_interactively_once(&mut self) -> Result<InteractiveExecutionResult, ShellError> {
        let mut shell = self.shell.lock().await;

        // Check for any completed jobs.
        shell.check_for_completed_jobs()?;

        // Run any pre-prompt commands.
        Self::run_pre_prompt_commands(&mut shell).await?;

        // Now that we've done that, compose the prompt.
        let mut prompt = InteractivePrompt {
            prompt: shell.compose_prompt().await?,
            alt_side_prompt: shell.compose_alt_side_prompt().await?,
            continuation_prompt: shell.compose_continuation_prompt().await?,
        };

        if let Some(terminal_integration) = &self.terminal_integration {
            let pre_prompt = terminal_integration.pre_prompt();
            let working_dir = terminal_integration.report_cwd(shell.working_dir());
            let post_prompt = terminal_integration.post_prompt();

            prompt.prompt = [
                pre_prompt.as_ref(),
                working_dir.as_ref(),
                prompt.prompt.as_str(),
                post_prompt.as_ref(),
            ]
            .concat();
        }

        drop(shell);

        match self.input.read_line(&self.shell, prompt)? {
            ReadResult::Input(read_result) => {
                self.execute_line(read_result, true /* user input */).await
            }
            ReadResult::BoundCommand(read_result) => {
                self.execute_line(read_result, false /* user input */).await
            }
            ReadResult::Eof => Ok(InteractiveExecutionResult::Eof),
            ReadResult::Interrupted => {
                let result: brush_core::ExecutionResult =
                    brush_core::ExecutionExitCode::Interrupted.into();
                self.shell
                    .lock()
                    .await
                    .set_last_exit_status(result.exit_code.into());
                Ok(InteractiveExecutionResult::Executed(result))
            }
        }
    }

    /// Executes the given line of input.
    ///
    /// # Arguments
    ///
    /// * `read_result` - The line of input to execute.
    /// * `user_input` - Whether the line came from direct user input (as opposed to a key binding, say).
    async fn execute_line(
        &mut self,
        read_result: String,
        user_input: bool,
    ) -> Result<InteractiveExecutionResult, ShellError> {
        let mut shell = self.shell.lock().await;

        // See if the the user interface has a non-empty read buffer.
        let buffer_info = self.input.get_read_buffer();

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

            // Invoke terminal integration.
            if let Some(terminal_integration) = &self.terminal_integration {
                print!(
                    "{}",
                    terminal_integration
                        .pre_exec_command(read_result.as_str())
                        .as_ref()
                );
                std::io::stdout().flush()?;
            }
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

        drop(shell);

        if buffer_and_cursor.is_none() && nonempty_buffer {
            buffer_and_cursor = Some((String::new(), 0));
        }

        if let Some((updated_buffer, updated_cursor)) = buffer_and_cursor {
            self.input.set_read_buffer(updated_buffer, updated_cursor);
        }

        // Invoke terminal integration.
        if let Some(terminal_integration) = &self.terminal_integration {
            let exit_code = result.as_ref().map_or(1, i32::from);
            print!(
                "{}",
                terminal_integration.post_exec_command(exit_code).as_ref()
            );
            std::io::stdout().flush()?;
        }

        result
    }

    async fn run_pre_prompt_commands(shell: &mut brush_core::Shell) -> Result<(), ShellError> {
        // If there's a variable called PROMPT_COMMAND, then run it first.
        if let Some(prompt_cmd_var) = shell.env_var("PROMPT_COMMAND") {
            match prompt_cmd_var.value() {
                brush_core::ShellValue::String(cmd_str) => {
                    Self::run_pre_prompt_command(shell, cmd_str.to_owned()).await?;
                }
                brush_core::ShellValue::IndexedArray(values) => {
                    let owned_values: Vec<_> = values.values().cloned().collect();
                    for cmd_str in owned_values {
                        Self::run_pre_prompt_command(shell, cmd_str).await?;
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
        prompt_cmd: String,
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
}
struct HostEnvironment;

impl crate::term::TerminalEnvironment for HostEnvironment {
    fn get_env_var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}
