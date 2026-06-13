use nu_ansi_term::Style;
use reedline::MenuBuilder;

use super::{completer, edit_mode, highlighter, history, validator};
use crate::{InputBackend, ReadResult, ShellError, input_backend::InteractivePrompt, refs};

/// Represents an interactive shell capable of taking commands from standard input
/// and reporting results to standard output and standard error streams.
pub struct ReedlineInputBackend {
    reedline: Option<reedline::Reedline>,
}

const COMPLETION_MENU_NAME: &str = "completion_menu";

fn completion_menu_text_style() -> Style {
    Style::new()
}

fn completion_menu_selected_text_style() -> Style {
    Style::new().bold().reverse()
}

fn completion_menu_match_text_style() -> Style {
    Style::new().underline()
}

fn completion_menu_selected_match_text_style() -> Style {
    completion_menu_selected_text_style().underline()
}

fn history_hint_style() -> Style {
    Style::new().italic().dimmed()
}

impl ReedlineInputBackend {
    /// Returns a new interactive shell instance, created with the provided options.
    ///
    /// # Arguments
    ///
    /// * `options` - Options for creating the input backend.
    /// * `shell_ref` - Shell that the backend will be used with.
    pub fn new(
        options: &crate::UIOptions,
        shell_ref: &refs::ShellRef<impl brush_core::ShellExtensions>,
    ) -> Result<Self, ShellError> {
        // Set up key bindings.
        let key_bindings = compose_key_bindings(COMPLETION_MENU_NAME);

        // Set up mutable edit mode.
        let mutable_edit_mode = edit_mode::MutableEditMode::new(key_bindings);
        let updatable_bindings = mutable_edit_mode.bindings();

        // Create helper objects that implement reedline traits; each will
        // hold a reference to the shell.
        let completer = completer::ReedlineCompleter {
            shell: shell_ref.clone(),
        };
        let validator = validator::ReedlineValidator {
            shell: shell_ref.clone(),
        };
        let syntax_highlighter = highlighter::ReedlineHighlighter {
            shell: shell_ref.clone(),
        };
        let history = history::ReedlineHistory {
            shell: shell_ref.clone(),
        };

        // Set up completion menu. Set an empty marker to avoid the
        // line's text horizontally shifting around during/after completion.
        // We set a max column count of 10 to ensure it's larger than the
        // hard-coded default (4 last we checked); if there's not enough
        // horizontal space in the terminal to fit that many columns, given
        // the actual text to be displayed, it will get effectively dereased
        // anyhow.
        let completion_menu = Box::new(
            reedline::ColumnarMenu::default()
                .with_name(COMPLETION_MENU_NAME)
                .with_marker("")
                .with_columns(10)
                .with_text_style(completion_menu_text_style())
                .with_match_text_style(completion_menu_match_text_style())
                .with_selected_text_style(completion_menu_selected_text_style())
                .with_selected_match_text_style(completion_menu_selected_match_text_style()),
        );

        // Set up default history-based hinter.
        let mut hinter = reedline::DefaultHinter::default();
        if !options.disable_color {
            hinter = hinter.with_style(history_hint_style());
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
            .with_edit_mode(Box::new(mutable_edit_mode))
            .with_history(Box::new(history));

        // Override Reedline's default example highlighter, which hard-codes white as the
        // neutral input color. When syntax highlighting is disabled we still install a plain
        // highlighter so typed text follows the terminal's default foreground color.
        if !options.disable_color {
            reedline = if options.disable_highlighting {
                reedline.with_highlighter(Box::new(highlighter::PlainTextHighlighter))
            } else {
                reedline.with_highlighter(Box::new(syntax_highlighter))
            };
        }

        let mut shell = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(shell_ref.lock())
        });

        shell.set_key_bindings(Some(updatable_bindings));
        drop(shell);

        Ok(Self {
            reedline: Some(reedline),
        })
    }
}

impl Drop for ReedlineInputBackend {
    fn drop(&mut self) {
        // It's unpleasant to need to do so, but if we detect a panic in the process of being
        // unwound, then we arrange for our reedline::Reedline instance to *not* get dropped.
        // Without this, then there's a chance that our panic handler emitted important
        // diagnostics to stdout but dropping the Reedline object will end up erasing it
        // when the latter object's internal Painter gets dropped and, in turn, may flush
        // some not-yet-flushed terminal control sequences. This isn't theoretical; we've
        // actively seen this in various cases where a panic occurs with Reedline::read_line()
        // on the stack.
        if std::thread::panicking() {
            let reedline = std::mem::take(&mut self.reedline);
            std::mem::forget(reedline);
        }
    }
}

impl InputBackend for ReedlineInputBackend {
    /// Reads a line of input, using the given prompt.
    ///
    /// # Arguments
    ///
    /// * `prompt` - The prompt to display to the user.
    fn read_line(
        &mut self,
        _shell: &crate::ShellRef<impl brush_core::ShellExtensions>,
        prompt: InteractivePrompt,
    ) -> Result<ReadResult, ShellError> {
        if let Some(reedline) = &mut self.reedline {
            match reedline.read_line(&prompt) {
                Ok(reedline::Signal::Success(s)) => {
                    if edit_mode::is_reedline_host_command(s.as_str()) {
                        Ok(ReadResult::BoundCommand(s))
                    } else {
                        Ok(ReadResult::Input(s))
                    }
                }
                Ok(reedline::Signal::CtrlC) => Ok(ReadResult::Interrupted),
                Ok(reedline::Signal::CtrlD) => Ok(ReadResult::Eof),
                Ok(reedline::Signal::ExternalBreak(_)) => Err(ShellError::UnexpectedInputFailure),
                Ok(_) => Err(ShellError::UnexpectedInputFailure),
                Err(err) => Err(ShellError::InputError(err)),
            }
        } else {
            Ok(ReadResult::Eof)
        }
    }

    fn get_read_buffer(&self) -> Option<(String, usize)> {
        self.reedline.as_ref().map(|r| {
            (
                r.current_buffer_contents().to_owned(),
                r.current_insertion_point(),
            )
        })
    }

    fn set_read_buffer(&mut self, buffer: String, cursor: usize) {
        if let Some(reedline) = &mut self.reedline {
            reedline.run_edit_commands(&[
                reedline::EditCommand::MoveToStart { select: false },
                reedline::EditCommand::ClearToLineEnd,
                reedline::EditCommand::InsertString(buffer),
                reedline::EditCommand::MoveToPosition {
                    position: cursor,
                    select: false,
                },
            ]);
        }
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
    // NOTE: To match readline, we bind Ctrl+_ to undo; in practice, the only way
    // to get that to work out is to specify Ctrl+7 for the binding. It's not clear
    // that this is terribly portable across terminals/environments.
    key_bindings.add_binding(
        reedline::KeyModifiers::CONTROL,
        reedline::KeyCode::Char('7'),
        reedline::ReedlineEvent::Edit(vec![reedline::EditCommand::Undo]),
    );

    // Capitalize.
    key_bindings.add_binding(
        reedline::KeyModifiers::ALT,
        reedline::KeyCode::Char('c'),
        reedline::ReedlineEvent::Edit(vec![
            reedline::EditCommand::CapitalizeChar,
            reedline::EditCommand::MoveWordRight { select: false },
        ]),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_hint_style_is_theme_adaptive() {
        let style = history_hint_style();

        assert_eq!(style.foreground, None);
        assert_eq!(style.background, None);
        assert!(style.is_italic);
        assert!(style.is_dimmed);
    }

    #[test]
    fn completion_menu_styles_are_theme_adaptive() {
        let text_style = completion_menu_text_style();
        let match_style = completion_menu_match_text_style();
        let selected_text_style = completion_menu_selected_text_style();
        let selected_match_text_style = completion_menu_selected_match_text_style();

        assert_eq!(text_style.foreground, None);
        assert_eq!(text_style.background, None);

        assert_eq!(match_style.foreground, None);
        assert_eq!(match_style.background, None);
        assert!(match_style.is_underline);

        assert_eq!(selected_text_style.foreground, None);
        assert_eq!(selected_text_style.background, None);
        assert!(selected_text_style.is_bold);
        assert!(selected_text_style.is_reverse);

        assert_eq!(selected_match_text_style.foreground, None);
        assert_eq!(selected_match_text_style.background, None);
        assert!(selected_match_text_style.is_bold);
        assert!(selected_match_text_style.is_reverse);
        assert!(selected_match_text_style.is_underline);
    }
}
