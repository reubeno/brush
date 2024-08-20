use rustyline::validate::ValidationResult;
use std::{
    borrow::Cow,
    io::Write,
    path::{Path, PathBuf},
};

type Editor = rustyline::Editor<EditorHelper, rustyline::history::FileHistory>;

/// Represents an interactive shell capable of taking commands from standard input
/// and reporting results to standard output and standard error streams.
pub struct InteractiveShell {
    /// The `rustyline` editor.
    editor: Editor,
    /// Optional path to the history file used for the shell.
    history_file_path: Option<PathBuf>,
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

    /// An error occurred while reading input.
    #[error("input error: {0}")]
    ReadlineError(#[from] rustyline::error::ReadlineError),
}

enum InteractiveExecutionResult {
    Executed(brush_core::ExecutionResult),
    Failed(brush_core::Error),
    Eof,
}

impl InteractiveShell {
    /// Returns a new interactive shell instance, created with the provided options.
    ///
    /// # Arguments
    ///
    /// * `options` - Options for creating the interactive shell.
    pub async fn new(options: &crate::Options) -> Result<InteractiveShell, ShellError> {
        // Set up shell first. Its initialization may influence how the
        // editor needs to operate.
        let shell = brush_core::Shell::new(&options.shell).await?;
        let history_file_path = shell.get_history_file_path();

        let mut editor = Self::new_editor(options, shell)?;
        if let Some(history_file_path) = &history_file_path {
            // If the history file doesn't already exist, then make a best-effort attempt.
            // to create it.
            if !history_file_path.exists() {
                let _ = std::fs::File::create(history_file_path);
            }

            // Make a best effort attempt to load the history file.
            let _ = editor.load_history(history_file_path);
        }

        Ok(InteractiveShell {
            editor,
            history_file_path,
        })
    }

    /// Returns an immutable reference to the inner shell object.
    pub fn shell(&self) -> &brush_core::Shell {
        &self.editor.helper().unwrap().shell
    }

    /// Returns a mutable reference to the inner shell object.
    pub fn shell_mut(&mut self) -> &mut brush_core::Shell {
        &mut self.editor.helper_mut().unwrap().shell
    }

    fn new_editor(
        options: &crate::Options,
        shell: brush_core::Shell,
    ) -> Result<Editor, ShellError> {
        let config = rustyline::config::Builder::new()
            .max_history_size(1000)?
            .history_ignore_dups(true)?
            .auto_add_history(true)
            .bell_style(rustyline::config::BellStyle::None)
            .completion_type(rustyline::config::CompletionType::List)
            .bracketed_paste(!options.disable_bracketed_paste)
            .build();

        let mut editor = rustyline::Editor::with_config(config)?;
        editor.set_helper(Some(EditorHelper::new(shell)));

        Ok(editor)
    }

    /// Runs the interactive shell loop, reading commands from standard input and writing
    /// results to standard output and standard error. Continues until the shell
    /// normally exits or until a fatal error occurs.
    pub async fn run_interactively(&mut self) -> Result<(), ShellError> {
        loop {
            // Check for any completed jobs.
            self.shell_mut().check_for_completed_jobs()?;

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

        if self.shell().options.interactive {
            writeln!(self.shell().stderr(), "exit")?;
        }

        if let Some(history_file_path) = &self.history_file_path {
            let history_result = if self.shell().options.append_to_history_file {
                self.editor.append_history(history_file_path)
            } else {
                self.editor.save_history(history_file_path)
            };

            if let Err(e) = history_result {
                // N.B. This seems like the sort of thing that's worth being noisy about,
                // but bash doesn't do that -- and probably for a reason.
                tracing::debug!(
                    "couldn't save history to {}: {e}",
                    history_file_path.display()
                );
            }
        }

        Ok(())
    }

    async fn run_interactively_once(&mut self) -> Result<InteractiveExecutionResult, ShellError> {
        // If there's a variable called PROMPT_COMMAND, then run it first.
        if let Some((_, prompt_cmd)) = self.shell().env.get("PROMPT_COMMAND") {
            let prompt_cmd = prompt_cmd.value().to_cow_string().to_string();

            // Save (and later restore) the last exit status.
            let prev_last_result = self.shell().last_exit_status;

            let params = self.shell().default_exec_params();

            self.shell_mut().run_string(prompt_cmd, &params).await?;

            self.shell_mut().last_exit_status = prev_last_result;
        }

        // Now that we've done that, compose the prompt.
        let prompt = self.shell_mut().compose_prompt().await?;

        match self.editor.readline(&prompt) {
            Ok(read_result) => {
                let params = self.shell().default_exec_params();
                match self.shell_mut().run_string(read_result, &params).await {
                    Ok(result) => Ok(InteractiveExecutionResult::Executed(result)),
                    Err(e) => Ok(InteractiveExecutionResult::Failed(e)),
                }
            }
            Err(rustyline::error::ReadlineError::Eof) => Ok(InteractiveExecutionResult::Eof),
            Err(rustyline::error::ReadlineError::Interrupted) => {
                self.shell_mut().last_exit_status = 130;
                Ok(InteractiveExecutionResult::Executed(
                    brush_core::ExecutionResult::new(130),
                ))
            }
            Err(e) => Err(e.into()),
        }
    }
}

//
// N.B. For now, we disable hinting on Windows because it sometimes results
// in prompt/input rendering errors.
//
#[cfg(unix)]
#[derive(rustyline::Helper, rustyline::Hinter)]
pub(crate) struct EditorHelper {
    pub shell: brush_core::Shell,

    #[rustyline(Hinter)]
    hinter: rustyline::hint::HistoryHinter,
}

#[cfg(windows)]
#[derive(rustyline::Helper)]
pub(crate) struct EditorHelper {
    pub shell: brush_core::Shell,
}

impl EditorHelper {
    #[cfg(unix)]
    pub(crate) fn new(shell: brush_core::Shell) -> Self {
        Self {
            shell,
            hinter: rustyline::hint::HistoryHinter::new(),
        }
    }

    #[cfg(windows)]
    pub(crate) fn new(shell: brush_core::Shell) -> Self {
        Self { shell }
    }

    fn get_completion_candidate_display_str(
        mut s: &str,
        options: &brush_core::completion::ProcessingOptions,
    ) -> String {
        let s_without_trailing_space = s.trim_end();
        let s_without_final_slash = s_without_trailing_space
            .strip_suffix(std::path::MAIN_SEPARATOR)
            .unwrap_or(s);

        if options.treat_as_filenames {
            if let Some(slash_index) = s_without_final_slash.rfind(std::path::MAIN_SEPARATOR) {
                s = &s[slash_index + 1..];
            }
        }

        s.to_owned()
    }

    async fn complete_async(
        &mut self,
        line: &str,
        pos: usize,
    ) -> rustyline::Result<(usize, Vec<rustyline::completion::Pair>)> {
        let working_dir = self.shell.working_dir.clone();

        // Intentionally ignore any errors that arise.
        let completion_future = self.shell.get_completions(line, pos);
        tokio::pin!(completion_future);

        // Wait for the completions to come back or interruption, whichever happens first.
        let result = loop {
            tokio::select! {
                result = &mut completion_future => {
                    break result;
                }
                _ = tokio::signal::ctrl_c() => {
                },
            }
        };

        let mut completions = result.unwrap_or_else(|_| brush_core::completion::Completions {
            start: pos,
            candidates: vec![],
            options: brush_core::completion::ProcessingOptions::default(),
        });

        let completing_end_of_line = pos == line.len();
        if completions.options.treat_as_filenames {
            for candidate in &mut completions.candidates {
                // Check if it's a directory.
                if !candidate.ends_with(std::path::MAIN_SEPARATOR) {
                    let candidate_path = Path::new(candidate);
                    let abs_candidate_path = if candidate_path.is_absolute() {
                        PathBuf::from(candidate_path)
                    } else {
                        working_dir.join(candidate_path)
                    };

                    if abs_candidate_path.is_dir() {
                        candidate.push(std::path::MAIN_SEPARATOR);
                    }
                }
            }
        }
        if completions.options.no_autoquote_filenames {
            tracing::debug!(target: "completion", "don't autoquote filenames");
        }
        if completing_end_of_line && !completions.options.no_trailing_space_at_end_of_line {
            for candidate in &mut completions.candidates {
                if !completions.options.treat_as_filenames
                    || !candidate.ends_with(std::path::MAIN_SEPARATOR)
                {
                    candidate.push(' ');
                }
            }
        }

        let options = completions.options;
        let candidates = completions
            .candidates
            .into_iter()
            .map(|c| rustyline::completion::Pair {
                display: Self::get_completion_candidate_display_str(c.as_str(), &options),
                replacement: c,
            })
            .collect();

        Ok((completions.start, candidates))
    }
}

impl rustyline::highlight::Highlighter for EditorHelper {
    // Display hints with low intensity
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Owned("\x1b[2m".to_owned() + hint + "\x1b[m")
    }

    // Color names that seem to be dirs.
    fn highlight_candidate<'c>(
        &self,
        candidate: &'c str, // FIXME should be Completer::Candidate
        _completion: rustyline::CompletionType,
    ) -> Cow<'c, str> {
        if let Some(candidate_without_suffix) = candidate.strip_suffix(std::path::MAIN_SEPARATOR) {
            Cow::Owned("\x1b[1;34m".to_owned() + candidate_without_suffix + "\x1b[0m/")
        } else {
            Cow::Borrowed(candidate)
        }
    }
}

impl rustyline::completion::Completer for EditorHelper {
    type Candidate = rustyline::completion::Pair;

    fn complete(
        &mut self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.complete_async(line, pos))
        })
    }
}

impl rustyline::validate::Validator for EditorHelper {
    fn validate(
        &self,
        ctx: &mut rustyline::validate::ValidationContext,
    ) -> rustyline::Result<rustyline::validate::ValidationResult> {
        let line = ctx.input();

        let parse_result = self.shell.parse_string(line.to_owned());

        let validation_result = match parse_result {
            Err(brush_parser::ParseError::Tokenizing { inner, position: _ })
                if inner.is_incomplete() =>
            {
                ValidationResult::Incomplete
            }
            Err(brush_parser::ParseError::ParsingAtEndOfInput) => ValidationResult::Incomplete,
            _ => ValidationResult::Valid(None),
        };

        Ok(validation_result)
    }
}

#[cfg(windows)]
impl rustyline::hint::Hinter for EditorHelper {
    type Hint = String;
}
