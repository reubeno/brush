use rustyline::validate::ValidationResult;
use std::{borrow::Cow, path::PathBuf};

use crate::{
    completion,
    error::ShellError,
    interactive_shell::{InteractivePrompt, InteractiveShell, ReadResult},
};

type Editor = rustyline::Editor<EditorHelper, rustyline::history::FileHistory>;

/// Represents an interactive shell capable of taking commands from standard input
/// and reporting results to standard output and standard error streams.
pub struct RustylineShell {
    /// The `rustyline` editor.
    editor: Editor,
    /// Optional path to the history file used for the shell.
    history_file_path: Option<PathBuf>,
}

impl RustylineShell {
    /// Returns a new interactive shell instance, created with the provided options.
    ///
    /// # Arguments
    ///
    /// * `options` - Options for creating the interactive shell.
    pub async fn new(options: &crate::Options) -> Result<RustylineShell, ShellError> {
        // Set up shell first. Its initialization may influence how the
        // editor needs to operate.
        let shell = brush_core::Shell::new(&options.shell).await?;
        let history_file_path = shell.get_history_file_path();

        let mut editor = Self::new_editor(options, shell).map_err(|_err| ShellError::InputError)?;
        if let Some(history_file_path) = &history_file_path {
            // If the history file doesn't already exist, then make a best-effort attempt.
            // to create it.
            if !history_file_path.exists() {
                let _ = std::fs::File::create(history_file_path);
            }

            // Make a best effort attempt to load the history file.
            let _ = editor.load_history(history_file_path);
        }

        Ok(RustylineShell {
            editor,
            history_file_path,
        })
    }

    fn new_editor(options: &crate::Options, shell: brush_core::Shell) -> rustyline::Result<Editor> {
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
}

impl InteractiveShell for RustylineShell {
    /// Returns an immutable reference to the inner shell object.
    fn shell(&self) -> impl AsRef<brush_core::Shell> {
        &self.editor.helper().unwrap().shell
    }

    /// Returns a mutable reference to the inner shell object.
    fn shell_mut(&mut self) -> impl AsMut<brush_core::Shell> {
        &mut self.editor.helper_mut().unwrap().shell
    }

    /// Reads a line of input, using the given prompt.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The prompt to display to the user.
    fn read_line(&mut self, prompt: InteractivePrompt) -> Result<ReadResult, ShellError> {
        match self.editor.readline(prompt.prompt.as_str()) {
            Ok(s) => Ok(ReadResult::Input(s)),
            Err(rustyline::error::ReadlineError::Eof) => Ok(ReadResult::Eof),
            Err(rustyline::error::ReadlineError::Interrupted) => Ok(ReadResult::Interrupted),
            Err(_err) => Err(ShellError::InputError),
        }
    }

    /// Update history, if relevant.
    fn update_history(&mut self) -> Result<(), ShellError> {
        if let Some(history_file_path) = &self.history_file_path {
            if self.shell().as_ref().options.append_to_history_file {
                self.editor
                    .append_history(history_file_path)
                    .map_err(|_err| ShellError::InputError)
            } else {
                self.editor
                    .save_history(history_file_path)
                    .map_err(|_err| ShellError::InputError)
            }
        } else {
            Ok(())
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
        let completions = completion::complete_async(&mut self.shell, line, pos).await;

        let options = completions.options;
        let candidates = completions
            .candidates
            .into_iter()
            .map(|c| rustyline::completion::Pair {
                display: Self::get_completion_candidate_display_str(c.as_str(), &options),
                replacement: c,
            })
            .collect();

        Ok((completions.insertion_index, candidates))
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
