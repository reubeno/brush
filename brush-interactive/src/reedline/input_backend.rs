use nu_ansi_term::Style;
use reedline::MenuBuilder;
use std::process::Command;

use brush_core::options::LineEditingMode;

use super::{completer, edit_mode, highlighter, history, validator};
use crate::{InputBackend, ReadResult, ShellError, input_backend::InteractivePrompt, refs};

/// Represents an interactive shell capable of taking commands from standard input
/// and reporting results to standard output and standard error streams.
pub struct ReedlineInputBackend {
    reedline: Option<reedline::Reedline>,
    pending_read_buffer: Option<(String, usize)>,
    updatable_bindings: std::sync::Arc<tokio::sync::Mutex<edit_mode::UpdatableBindings>>,
    configured_mode: Option<(LineEditingMode, u64)>,
    installed_bindings_revision: Option<u64>,
    editor_directory: Option<tempfile::TempDir>,
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
        let emacs_key_bindings = compose_emacs_key_bindings(COMPLETION_MENU_NAME);
        let vi_insert_key_bindings = compose_vi_insert_key_bindings(COMPLETION_MENU_NAME);
        let vi_command_key_bindings = compose_vi_command_key_bindings();

        // The shared object stores configuration only. Reedline owns the live editors.
        let updatable_bindings =
            std::sync::Arc::new(tokio::sync::Mutex::new(edit_mode::UpdatableBindings::new(
                emacs_key_bindings,
                vi_insert_key_bindings,
                vi_command_key_bindings,
            )));

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
                // Like readline, hand completion the whole line, not just the text before
                // the cursor: completion functions see the words after the cursor (e.g. in
                // COMP_WORDS), and candidates completed mid-line don't get a trailing space.
                .with_input_mode(reedline::InputMode::FullBuffer)
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

        shell.set_key_bindings(Some(updatable_bindings.clone()));
        drop(shell);

        Ok(Self {
            reedline: Some(reedline),
            pending_read_buffer: None,
            updatable_bindings,
            configured_mode: None,
            installed_bindings_revision: None,
            editor_directory: None,
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
    #[allow(
        clippy::unused_async_trait_impl,
        reason = "this backend reads synchronously; the trait is async for those that don't"
    )]
    async fn read_line(
        &mut self,
        shell: &crate::ShellRef<impl brush_core::ShellExtensions>,
        prompt: InteractivePrompt,
    ) -> Result<ReadResult, ShellError> {
        let requested = {
            let shell = shell.lock().await;
            (
                shell.options().line_editing_mode(),
                shell.options().line_editing_mode_revision,
            )
        };

        if matches!(requested.0, LineEditingMode::None) {
            self.configured_mode = Some(requested);
            return crate::MinimalInputBackend.read_line(shell, prompt).await;
        }

        let editor_command = configure_buffer_editor(&*shell.lock().await)?;
        let directory = if let Some(directory) = &self.editor_directory {
            directory
        } else {
            self.editor_directory
                .insert(tempfile::tempdir().map_err(ShellError::InputError)?)
        };
        let path = directory.path().join("buffer");
        // Acquire shared configuration before taking ownership of the editor:
        // cancellation while waiting for a lock must not discard the editor.
        let mut bindings = self.updatable_bindings.lock().await;
        let Some(mut reedline) = self.reedline.take() else {
            return Ok(ReadResult::Eof);
        };

        let revision = bindings.revision();
        if self.configured_mode != Some(requested)
            || self.installed_bindings_revision != Some(revision)
        {
            let active = if self.configured_mode != Some(requested) {
                match requested.0 {
                    LineEditingMode::Vi => {
                        reedline::PromptEditMode::Vi(reedline::PromptViMode::Insert)
                    }
                    _ => reedline::PromptEditMode::Emacs,
                }
            } else {
                reedline.prompt_edit_mode()
            };
            bindings.set_current_mode(&active);
            let (active_editor, standby) = bindings.editors(active);
            reedline = reedline
                .clear_edit_modes()
                .with_edit_mode(active_editor)
                .with_additional_edit_mode(standby);
        }
        drop(bindings);
        self.installed_bindings_revision = Some(revision);
        self.configured_mode = Some(requested);
        reedline = reedline.with_buffer_editor(editor_command, path);
        // Restore callback state under the requested mode. Applying it earlier
        // can clamp an insert-mode end cursor while vi normal is still active.
        if let Some((buffer, cursor)) = self.pending_read_buffer.take() {
            reedline.run_edit_commands(&[
                reedline::EditCommand::Clear,
                reedline::EditCommand::InsertString(buffer),
                reedline::EditCommand::MoveToPosition {
                    position: cursor,
                    select: false,
                },
            ]);
        }
        let result = reedline.read_line(&prompt);
        let mode = reedline.prompt_edit_mode();
        self.reedline = Some(reedline);
        self.updatable_bindings.lock().await.set_current_mode(&mode);
        match result {
            Ok(reedline::Signal::Success(s)) => Ok(ReadResult::Input(s)),
            Ok(reedline::Signal::CtrlC) => Ok(ReadResult::Interrupted),
            Ok(reedline::Signal::CtrlD) => Ok(ReadResult::Eof),
            Ok(reedline::Signal::HostCommand(cmd)) => Ok(ReadResult::BoundCommand(cmd)),
            Ok(_) => Err(ShellError::UnexpectedInputFailure),
            Err(err) => Err(ShellError::InputError(err)),
        }
    }

    fn get_read_buffer(&self) -> Option<(String, usize)> {
        self.pending_read_buffer.clone().or_else(|| {
            self.reedline.as_ref().map(|r| {
                (
                    r.current_buffer_contents().to_owned(),
                    r.current_insertion_point(),
                )
            })
        })
    }

    fn set_read_buffer(&mut self, buffer: String, cursor: usize) {
        self.pending_read_buffer = Some((buffer, cursor));
    }
}

fn configure_buffer_editor(
    shell: &brush_core::Shell<impl brush_core::ShellExtensions>,
) -> Result<Command, ShellError> {
    let editor = shell
        .env_str("VISUAL")
        .filter(|value| !value.is_empty())
        .or_else(|| shell.env_str("EDITOR").filter(|value| !value.is_empty()))
        .unwrap_or_else(|| "vi".into());

    let mut command = parse_editor_command(&editor).map_err(ShellError::InputError)?;
    command.current_dir(shell.working_dir()).env_clear();
    for (name, variable) in shell.env().iter_exported() {
        if variable.value().is_set() {
            command.env(name, variable.value().to_cow_str(shell).as_ref());
        }
    }
    Ok(command)
}

fn parse_editor_command(editor: &str) -> Result<Command, std::io::Error> {
    let invalid = || {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "editor must be a program and literal quoted arguments",
        )
    };
    if editor.contains('\0') {
        return Err(invalid());
    }
    let words = brush_parser::tokenize_str(editor)
        .map_err(|_| invalid())?
        .into_iter()
        .map(|token| match token {
            brush_parser::Token::Word(word, _) => Ok(brush_parser::unquote_str(&word)),
            brush_parser::Token::Operator(..) => Err(invalid()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (program, args) = words
        .split_first()
        .filter(|(program, _)| !program.is_empty())
        .ok_or_else(invalid)?;
    let mut command = Command::new(program);
    command.args(args);
    Ok(command)
}

fn add_completion_bindings(key_bindings: &mut reedline::Keybindings, completion_menu_name: &str) {
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
}

fn add_custom_common_bindings(key_bindings: &mut reedline::Keybindings) {
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
}

fn compose_emacs_key_bindings(completion_menu_name: &str) -> reedline::Keybindings {
    let mut key_bindings = reedline::default_emacs_keybindings();

    add_completion_bindings(&mut key_bindings, completion_menu_name);
    add_custom_common_bindings(&mut key_bindings);

    key_bindings
}

fn compose_vi_insert_key_bindings(completion_menu_name: &str) -> reedline::Keybindings {
    let mut key_bindings = reedline::default_vi_insert_keybindings();

    add_completion_bindings(&mut key_bindings, completion_menu_name);
    add_custom_common_bindings(&mut key_bindings);

    // Bash vi-insert does not bind clear-screen by default.
    let _ = key_bindings.remove_binding(
        reedline::KeyModifiers::CONTROL,
        reedline::KeyCode::Char('l'),
    );

    key_bindings
}

fn compose_vi_command_key_bindings() -> reedline::Keybindings {
    reedline::default_vi_normal_keybindings()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn latest_pending_callback_buffer_is_visible_before_resuming_input() {
        let shell = brush_core::Shell::builder().build().await.unwrap();
        let shell = std::sync::Arc::new(tokio::sync::Mutex::new(shell));
        let mut backend = ReedlineInputBackend::new(&crate::UIOptions::default(), &shell).unwrap();
        backend.set_read_buffer("abc".into(), 3);
        assert_eq!(backend.get_read_buffer(), Some(("abc".into(), 3)));

        // Each update supersedes the pending state, even when clearing it.
        backend.set_read_buffer("next".into(), 2);
        assert_eq!(backend.get_read_buffer(), Some(("next".into(), 2)));
        backend.set_read_buffer(String::new(), 0);
        assert_eq!(backend.get_read_buffer(), Some((String::new(), 0)));
    }

    #[test]
    fn editor_arguments_are_unquoted_but_never_expanded() {
        let command = parse_editor_command(
            r#"'/editor path' --flag 'two words' "" '$HOME' '$(touch sentinel)' a\ b"#,
        )
        .unwrap();
        assert_eq!(command.get_program(), "/editor path");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "--flag",
                "two words",
                "",
                "$HOME",
                "$(touch sentinel)",
                "a b"
            ]
        );
    }

    #[test]
    fn malformed_editor_commands_are_rejected() {
        for editor in [
            "",
            "''",
            "vi 'unterminated",
            "vi; echo no",
            "vi | cat",
            "vi > file",
            "vi\0",
            "vi 'a\0b'",
        ] {
            assert!(parse_editor_command(editor).is_err(), "accepted {editor:?}");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn editor_configuration_errors_do_not_consume_the_editor() {
        let mut shell = brush_core::Shell::builder().build().await.unwrap();
        shell
            .options_mut()
            .set_line_editing_mode(LineEditingMode::Emacs);
        let result = shell
            .run_string(
                r#"VISUAL="vi '""#,
                &brush_core::SourceInfo::from("test"),
                &shell.default_exec_params(),
            )
            .await
            .unwrap();
        assert!(matches!(
            result.exit_code,
            brush_core::ExecutionExitCode::Success
        ));
        let shell = std::sync::Arc::new(tokio::sync::Mutex::new(shell));
        let mut backend = ReedlineInputBackend::new(&crate::UIOptions::default(), &shell).unwrap();
        backend.set_read_buffer("callback result".into(), 15);
        let prompt = InteractivePrompt {
            prompt: String::new(),
            alt_side_prompt: String::new(),
            continuation_prompt: String::new(),
        };
        assert!(
            matches!(backend.read_line(&shell, prompt).await, Err(ShellError::InputError(error)) if error.kind() == std::io::ErrorKind::InvalidInput)
        );
        assert!(backend.reedline.is_some());
        assert!(backend.editor_directory.is_none());
        assert_eq!(
            backend.get_read_buffer(),
            Some(("callback result".into(), 15))
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cancelling_configuration_keeps_the_editor() {
        use std::{
            future::Future as _,
            task::{Context, Poll, Waker},
        };

        let mut shell = brush_core::Shell::builder().build().await.unwrap();
        shell
            .options_mut()
            .set_line_editing_mode(LineEditingMode::Emacs);
        let shell = std::sync::Arc::new(tokio::sync::Mutex::new(shell));
        let mut backend = ReedlineInputBackend::new(&crate::UIOptions::default(), &shell).unwrap();
        backend.set_read_buffer("callback result".into(), 15);
        let bindings = backend.updatable_bindings.clone();
        let _guard = bindings.lock().await;
        {
            let prompt = InteractivePrompt {
                prompt: String::new(),
                alt_side_prompt: String::new(),
                continuation_prompt: String::new(),
            };
            let mut read = std::pin::pin!(backend.read_line(&shell, prompt));
            assert!(matches!(
                read.as_mut().poll(&mut Context::from_waker(Waker::noop())),
                Poll::Pending
            ));
        }
        assert!(backend.reedline.is_some());
        assert_eq!(
            backend.get_read_buffer(),
            Some(("callback result".into(), 15))
        );
    }

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
