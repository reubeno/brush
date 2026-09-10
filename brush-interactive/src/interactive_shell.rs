use std::io::IsTerminal as _;
use std::io::Write as _;
use std::ops::ControlFlow;

use crate::InputBackend;
use crate::InteractivePrompt;
use crate::ReadResult;
use crate::ShellError;

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

/// Options for interactive shells.
#[derive(Clone, Default)]
pub struct InteractiveOptions {
    /// Whether terminal shell integration is enabled.
    pub terminal_shell_integration: bool,
    /// Whether to run zsh-style `precmd_functions`/`preexec_functions` hooks. Inert on its
    /// own: the embedder must also call [`init_zsh_style_hooks`](crate::init_zsh_style_hooks)
    /// before the shell loads its profile and rc files.
    pub zsh_style_hooks: bool,
}

/// Reads `PROMPT_COMMAND` as bash does: a string is a single command, an indexed array is one
/// command per element, and any other type runs nothing at all. Deliberately *not* read as a
/// word list the way the hook registries are: bash gives `PROMPT_COMMAND` its own handling
/// rather than expanding it as `"${PROMPT_COMMAND[@]}"`.
fn read_prompt_commands<SE: brush_core::ShellExtensions>(
    shell: &brush_core::Shell<SE>,
) -> Vec<String> {
    match shell.env_var("PROMPT_COMMAND").map(|var| var.value()) {
        Some(brush_core::ShellValue::String(cmd)) => vec![cmd.to_owned()],
        Some(brush_core::ShellValue::IndexedArray(cmds)) => cmds.values().cloned().collect(),
        _ => vec![],
    }
}

/// Represents an interactive shell that displays prompts, interactively reads user input, etc.
pub struct InteractiveShell<'a, IB: InputBackend, SE: brush_core::ShellExtensions> {
    /// The underlying shell instance.
    shell: crate::ShellRef<SE>,
    /// The input backend to use.
    input: &'a mut IB,
    /// Terminal integration utility; inert if integration is off or unsupported.
    terminal_integration: crate::term_integration::TerminalIntegration,
    /// Terminal-control guard, held for the lifetime of the interactive shell.
    _terminal_control: Option<brush_core::terminal::TerminalControl>,
    /// Options.
    options: InteractiveOptions,
}

impl<'a, IB: InputBackend, SE: brush_core::ShellExtensions> InteractiveShell<'a, IB, SE> {
    /// Creates a new `InteractiveShell` wrapping the given shell instance.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell instance to wrap.
    /// * `input` - The input backend to use.
    /// * `options` - The user interface options to use.
    pub fn new(
        shell: &crate::ShellRef<SE>,
        input: &'a mut IB,
        options: &InteractiveOptions,
    ) -> Result<Self, ShellError> {
        let stdin_is_terminal = std::io::stdin().is_terminal();

        // Acquire terminal control if stdin is a terminal.
        let terminal_control = if stdin_is_terminal {
            Some(brush_core::terminal::TerminalControl::acquire()?)
        } else {
            None
        };

        // Set up terminal integration if enabled *and* if stdin is a terminal. Otherwise a
        // switched-off one, which every call site below can report events to unconditionally.
        let terminal_integration = if options.terminal_shell_integration && stdin_is_terminal {
            let terminfo = crate::term_detection::get_terminal_info(&HostEnvironment);
            crate::term_integration::TerminalIntegration::init(terminfo)?
        } else {
            crate::term_integration::TerminalIntegration::disabled()
        };

        Ok(Self {
            shell: shell.clone(),
            input,
            terminal_integration,
            _terminal_control: terminal_control,
            options: options.clone(),
        })
    }

    /// Runs the interactive shell loop, reading commands from standard input and writing
    /// results to standard output and standard error. Continues until the shell
    /// normally exits or until a fatal error occurs.
    pub async fn run_interactively(&mut self) -> Result<(), ShellError> {
        let mut shell = self.shell.lock().await;

        let mut announce_exit = shell.options().interactive;

        shell.start_interactive_session()?;

        drop(shell);

        loop {
            let result = self.run_interactively_once().await?;
            match result {
                InteractiveExecutionResult::Executed(result) if result.is_exit() => {
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
                    let _ = shell.display_error(&mut stderr, &err);

                    drop(shell);
                }
                InteractiveExecutionResult::Eof => {
                    break;
                }
            }

            if self.shell.lock().await.options().exit_after_one_command {
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

        // Everything between here and reading input is prompt work, and a shell that displays
        // no prompt does none of it: `script | brush -s` reads commands through this loop but
        // isn't interactive in the `$-` sense, so bash runs no PROMPT_COMMAND and expands no
        // PS1 there.
        let prompt = if shell.options().interactive {
            // Run any pre-prompt actions.
            if let ControlFlow::Break(exit) =
                Self::run_pre_prompt_actions(&mut shell, &self.options).await?
            {
                return Ok(InteractiveExecutionResult::Executed(exit));
            }

            // Compose the prompt.
            Self::compose_prompt(&mut shell, &self.terminal_integration).await?
        } else {
            InteractivePrompt::default()
        };

        drop(shell);

        // Read input.
        match self.input.read_line(&self.shell, prompt)? {
            ReadResult::Input(read_result) => {
                // We got a line of input -- execute it.
                self.execute_line(read_result, true /* user input */).await
            }
            ReadResult::BoundCommand(read_result) => {
                // We got a line that was bound to keybindings; execute it.
                self.execute_line(read_result, false /* user input */).await
            }
            ReadResult::Eof => {
                // We're done!
                Ok(InteractiveExecutionResult::Eof)
            }
            ReadResult::Interrupted => {
                // We were interrupted; report that appropriately.
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

    async fn compose_prompt(
        shell: &mut brush_core::Shell<SE>,
        terminal_integration: &crate::term_integration::TerminalIntegration,
    ) -> Result<InteractivePrompt, ShellError> {
        // Now that we've done that, compose the prompt.
        let mut prompt = InteractivePrompt {
            prompt: shell.compose_prompt().await?,
            alt_side_prompt: shell.compose_alt_side_prompt().await?,
            continuation_prompt: shell.compose_continuation_prompt().await?,
        };

        prompt.prompt = terminal_integration.decorate_prompt(prompt.prompt, shell.working_dir());

        Ok(prompt)
    }

    /// Executes the given line of input.
    ///
    /// # Arguments
    ///
    /// * `read_result` - The line of input to execute.
    /// * `user_input` - Whether the line came from direct user input (as opposed to a key binding,
    ///   say).
    async fn execute_line(
        &mut self,
        read_result: String,
        user_input: bool,
    ) -> Result<InteractiveExecutionResult, ShellError> {
        let mut shell = self.shell.lock().await;

        // See if the the user interface has a non-empty read buffer.
        let buffer_info = self.input.get_read_buffer();

        // If the user interface has a read buffer -- even an empty one -- reflect it to the
        // shell so that bound commands see READLINE_LINE/READLINE_POINT, as they do in bash.
        let had_buffer = if let Some((buffer, cursor)) = buffer_info {
            shell.set_edit_buffer(buffer, cursor)?;
            true
        } else {
            false
        };

        // If the line came from direct user input (as opposed to a key binding, say), then we
        // need to do a few more things before executing it. A hook that exited the shell
        // stands in for the command: the line never runs, and neither does the bookkeeping
        // below, which only matters to a shell that goes on to read another line.
        if user_input
            && let ControlFlow::Break(exit) =
                Self::run_pre_exec_actions(&mut shell, read_result.as_str(), &self.options).await?
        {
            return Ok(InteractiveExecutionResult::Executed(exit));
        }

        // Count the command's lines.
        let line_count = read_result.lines().count().max(1);

        // Terminal integration brackets the command with a matched pair of markers, so both
        // are emitted here, around the one call that runs it: no failure between them can
        // leave a started command unclosed, and nothing below this writes to the terminal.
        self.terminal_integration
            .on_pre_exec_command(&read_result)?;

        // Execute the command.
        let params = shell.default_exec_params();
        let source_info = brush_core::SourceInfo::from("main");
        let result = match shell.run_string(read_result, &source_info, &params).await {
            Ok(result) => Ok(InteractiveExecutionResult::Executed(result)),
            Err(e) => Ok(InteractiveExecutionResult::Failed(e)),
        };

        self.terminal_integration
            .on_post_exec_command(result.as_ref().map_or(1, i32::from))?;

        // Update cumulative line counter based on actual lines in the command.
        shell.increment_interactive_line_offset(line_count);

        // See if the shell has input buffer state that we need to reflect back to
        // the user interface. It may be state that originally came from the user
        // interface, or it may be state that was programmatically generated by
        // the command we just executed.
        let mut buffer_and_cursor = shell.pop_edit_buffer()?;

        drop(shell);

        if buffer_and_cursor.is_none() && had_buffer {
            buffer_and_cursor = Some((String::new(), 0));
        }

        if let Some((updated_buffer, updated_cursor)) = buffer_and_cursor {
            self.input.set_read_buffer(updated_buffer, updated_cursor);
        }

        result
    }

    /// Runs pre-prompt actions. Breaks with the result of one that exited the shell.
    async fn run_pre_prompt_actions(
        shell: &mut brush_core::Shell<SE>,
        options: &InteractiveOptions,
    ) -> Result<ControlFlow<brush_core::ExecutionResult>, ShellError> {
        // precmd hooks first: bash-preexec prepends its dispatcher to PROMPT_COMMAND.
        if let ControlFlow::Break(exit) = crate::zsh_hooks::run_precmd(shell, options).await? {
            return Ok(ControlFlow::Break(exit));
        }

        // Next, if there's a variable called PROMPT_COMMAND, then run it.
        for prompt_cmd in read_prompt_commands(shell) {
            if let ControlFlow::Break(exit) =
                Self::run_pre_prompt_command(shell, prompt_cmd).await?
            {
                return Ok(ControlFlow::Break(exit));
            }
        }

        Ok(ControlFlow::Continue(()))
    }

    /// Runs pre-exec actions. Breaks with the result of a hook that exited the shell.
    ///
    /// # Arguments
    ///
    /// * `shell` - The shell to run the actions in.
    /// * `command_line` - The line as entered by the user.
    /// * `options` - The options the interactive loop is running with.
    async fn run_pre_exec_actions(
        shell: &mut brush_core::Shell<SE>,
        command_line: &str,
        options: &InteractiveOptions,
    ) -> Result<ControlFlow<brush_core::ExecutionResult>, ShellError> {
        // Display the pre-command prompt on stderr (if there is one). Like the other prompts,
        // this is expanded only by a shell that's interactive in the `$-` sense.
        if shell.options().interactive {
            let precmd_prompt = shell.compose_precmd_prompt().await?;
            if !precmd_prompt.is_empty() {
                eprint!("{precmd_prompt}");
                std::io::stderr().flush()?;
            }
        }

        // Update history (if applicable).
        shell.add_to_history(command_line.trim_end_matches('\n'))?;

        // preexec hooks get the line as entered; they are the last thing before it runs, so
        // their break is this function's.
        crate::zsh_hooks::run_preexec(shell, options, command_line).await
    }

    /// Runs one `PROMPT_COMMAND` entry. Breaks with its result if it exited the shell.
    async fn run_pre_prompt_command(
        shell: &mut brush_core::Shell<SE>,
        prompt_cmd: String,
    ) -> Result<ControlFlow<brush_core::ExecutionResult>, ShellError> {
        let saved_status = shell.save_command_status();

        // Run the command.
        let params = shell.default_exec_params();
        let source_info = brush_core::SourceInfo::from("PROMPT_COMMAND");
        let result = shell.run_string(prompt_cmd, &source_info, &params).await?;
        if result.is_exit() {
            return Ok(ControlFlow::Break(result));
        }

        shell.restore_command_status(saved_status);

        Ok(ControlFlow::Continue(()))
    }
}

/// Represents the host environment; used for terminal detection in conjunction
/// with the `TerminalEnvironment` trait.
struct HostEnvironment;

impl crate::term_detection::TerminalEnvironment for HostEnvironment {
    /// Gets the value of the given environment variable from the host process's
    /// OS environment variables. Returns `None` if the variable is not set.
    ///
    /// # Arguments
    ///
    /// * `name` - The name of the environment variable to get.
    fn get_env_var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}
