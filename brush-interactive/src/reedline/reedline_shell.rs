use nu_ansi_term::Color;
use reedline::MenuBuilder;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::{completer, highlighter, refs, validator};
use crate::{interactive_shell::InteractivePrompt, InteractiveShell, ReadResult, ShellError};

/// Represents an interactive shell capable of taking commands from standard input
/// and reporting results to standard output and standard error streams.
pub struct ReedlineShell {
    reedline: reedline::Reedline,
    shell: refs::ShellRef,
}

const COMPLETION_MENU_NAME: &str = "completion_menu";

impl ReedlineShell {
    /// Returns a new interactive shell instance, created with the provided options.
    ///
    /// # Arguments
    ///
    /// * `options` - Options for creating the interactive shell.
    pub async fn new(options: &crate::Options) -> Result<ReedlineShell, ShellError> {
        // Set up shell first. Its initialization may influence how the
        // editor needs to operate.
        let shell = brush_core::Shell::new(&options.shell).await?;
        let history_file_path = shell.get_history_file_path();

        let shell_ref = Arc::new(Mutex::new(shell));

        // Create helper objects that implement reedline traits; each will
        // hold a reference to the shell.
        let completer = completer::ReedlineCompleter {
            shell: shell_ref.clone(),
        };
        let validator = validator::ReedlineValidator {
            shell: shell_ref.clone(),
        };
        let highlighter = highlighter::ReedlineHighlighter {
            shell: shell_ref.clone(),
        };

        // Set up completion menu. Set an empty marker to avoid the
        // line's text horizontally shifting around during/after completion.
        let completion_menu = Box::new(
            reedline::ColumnarMenu::default()
                .with_name(COMPLETION_MENU_NAME)
                .with_marker("")
                .with_selected_text_style(Color::Blue.bold().reverse())
                .with_selected_match_text_style(Color::Blue.bold().reverse()),
        );

        // Set up key bindings.
        let key_bindings = compose_key_bindings(COMPLETION_MENU_NAME);

        // Set up default history-based hinter.
        let mut hinter = reedline::DefaultHinter::default();
        if !options.disable_color {
            hinter = hinter.with_style(nu_ansi_term::Style::new().italic().fg(Color::DarkGray));
        }

        // Instantiate reedline with some defaults and hand it ownership of
        // the helpers.
        let mut reedline = reedline::Reedline::create()
            .with_ansi_colors(!options.disable_color)
            .use_bracketed_paste(!options.disable_bracketed_paste)
            .with_completer(Box::new(completer))
            .with_quick_completions(true)
            .with_validator(Box::new(validator))
            .with_hinter(Box::new(hinter))
            .with_menu(reedline::ReedlineMenu::EngineCompleter(completion_menu))
            .with_edit_mode(Box::new(reedline::Emacs::new(key_bindings)));

        // If requested, apply some additional niceties.
        if !options.disable_highlighting && !options.disable_color {
            reedline = reedline.with_highlighter(Box::new(highlighter));
        }

        // If we have a history file, wire it up.
        if let Some(history_file_path) = history_file_path {
            if let Ok(history) =
                reedline::FileBackedHistory::with_file(reedline::HISTORY_SIZE, history_file_path)
            {
                reedline = reedline.with_history(Box::new(history));
            }
        }

        Ok(ReedlineShell {
            reedline,
            shell: shell_ref,
        })
    }
}

impl InteractiveShell for ReedlineShell {
    /// Returns an immutable reference to the inner shell object.
    fn shell(&self) -> impl AsRef<brush_core::Shell> + Send {
        refs::ReedlineShellReader {
            shell: self.shell.try_lock().unwrap(),
        }
    }

    /// Returns a mutable reference to the inner shell object.
    fn shell_mut(&mut self) -> impl AsMut<brush_core::Shell> + Send {
        refs::ReedlineShellWriter {
            shell: self.shell.try_lock().unwrap(),
        }
    }

    /// Reads a line of input, using the given prompt.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The prompt to display to the user.
    fn read_line(&mut self, prompt: InteractivePrompt) -> Result<ReadResult, ShellError> {
        match self.reedline.read_line(&prompt) {
            Ok(reedline::Signal::Success(s)) => Ok(ReadResult::Input(s)),
            Ok(reedline::Signal::CtrlC) => Ok(ReadResult::Interrupted),
            Ok(reedline::Signal::CtrlD) => Ok(ReadResult::Eof),
            Err(err) => Err(ShellError::IoError(err)),
        }
    }

    /// Update history, if relevant.
    fn update_history(&mut self) -> Result<(), ShellError> {
        // N.B. With our current usage, reedline auto-updates the history file.
        Ok(())
    }
}

fn compose_key_bindings(completion_menu_name: &str) -> reedline::Keybindings {
    let mut key_bindings = reedline::default_emacs_keybindings();

    // Wire up tab to completion.
    key_bindings.add_binding(
        reedline::KeyModifiers::NONE,
        reedline::KeyCode::Tab,
        reedline::ReedlineEvent::UntilFound(vec![
            reedline::ReedlineEvent::Menu(completion_menu_name.to_string()),
            reedline::ReedlineEvent::MenuNext,
            reedline::ReedlineEvent::Edit(vec![reedline::EditCommand::Complete]),
        ]),
    );
    // Wire up shift-tab for completion.
    key_bindings.add_binding(
        reedline::KeyModifiers::SHIFT,
        reedline::KeyCode::BackTab,
        reedline::ReedlineEvent::MenuPrevious,
    );

    // Add undo.
    // TODO: We would prefer Ctrl+_ to match readline, but that doesn't seem to work.
    key_bindings.add_binding(
        reedline::KeyModifiers::ALT,
        reedline::KeyCode::Char('_'),
        reedline::ReedlineEvent::Edit(vec![reedline::EditCommand::Undo]),
    );

    // Add comment.
    key_bindings.add_binding(
        reedline::KeyModifiers::ALT,
        reedline::KeyCode::Char('#'),
        reedline::ReedlineEvent::Multiple(vec![
            reedline::ReedlineEvent::Edit(vec![
                reedline::EditCommand::MoveToStart { select: false },
                reedline::EditCommand::InsertChar('#'),
            ]),
            reedline::ReedlineEvent::Enter,
        ]),
    );

    key_bindings
}
