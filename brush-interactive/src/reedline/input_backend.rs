use brush_core::trace_categories;
use nu_ansi_term::Style;
use reedline::MenuBuilder;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::{
    completer, edit_mode, events, events::HostCommand, highlighter, history,
    pending::DeferredReplay, validator,
};
use crate::{InputBackend, ReadResult, ShellError, input_backend::InteractivePrompt, refs};

/// Represents an interactive shell capable of taking commands from standard input
/// and reporting results to standard output and standard error streams.
pub struct ReedlineInputBackend {
    reedline: Option<reedline::Reedline>,
    bindings: Arc<Mutex<edit_mode::UpdatableBindings>>,
    /// Macro bytes to replay at the next read, left behind by the bound command just returned.
    pending_replay: Option<DeferredReplay>,
}

const COMPLETION_MENU_NAME: &str = "completion_menu";

/// How many times `reedline.read_line()` is attempted before its error is
/// propagated: the initial call plus this many minus one retries. See
/// `read_line` below.
const MAX_READ_LINE_ATTEMPTS: u32 = 3;

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

        shell.set_key_bindings(Some(updatable_bindings.clone()));
        drop(shell);

        Ok(Self {
            reedline: Some(reedline),
            bindings: updatable_bindings,
            pending_replay: None,
        })
    }

    fn lock_bindings(&self) -> tokio::sync::MutexGuard<'_, edit_mode::UpdatableBindings> {
        lock(&self.bindings)
    }

    /// Settles a read. The read is ended on the bindings, dropping any key held for a
    /// sequence, whether the read succeeded or not. A stop reached through a macro comes back
    /// from reedline as the index of a record saying what to do: a bound command is returned
    /// as the command itself with the bytes after it as the pending replay, and an
    /// accept-line asks for the buffer to be accepted now with those bytes replayed after.
    fn settle(&mut self, result: Result<ReadResult, ShellError>) -> Result<Settled, ShellError> {
        self.pending_replay = None;

        // What reedline returned, if it was one of our own encodings rather than a command
        // the user bound.
        let host = match &result {
            Ok(ReadResult::BoundCommand(command)) => Some(HostCommand::decode(command)),
            _ => None,
        };
        // End the read whether or not it succeeded: claim the record it returned, if any,
        // and drop the rest.
        let claimed = self.lock_bindings().end_read(match &host {
            Some(HostCommand::Deferred(id)) => Some(*id),
            _ => None,
        });
        let result = result?;

        let deferred = match host {
            Some(HostCommand::Deferred(_)) => claimed,
            Some(other) => return Ok(settled_for(other)),
            None => return Ok(Settled::Return(result)),
        };
        let Some(deferred) = deferred else {
            // A bookkeeping bug: the record was never recorded or was already claimed.
            // Prompt again rather than end the session over it.
            tracing::debug!(
                target: trace_categories::INPUT,
                "reedline returned a deferred record we no longer hold"
            );
            return Ok(Settled::Resume);
        };

        Ok(match deferred.action {
            events::DeferredAction::RunCommand(command) => {
                self.pending_replay = deferred.replay;
                settled_for(command)
            }
            events::DeferredAction::AcceptLine => Settled::Accept(deferred.replay),
        })
    }

    /// Reads a line and settles it.
    fn read_and_settle(
        &mut self,
        prompt: &InteractivePrompt,
        accept_immediately: bool,
    ) -> Result<Settled, ShellError> {
        let Some(signal) = self.read_line_from_reedline(prompt, accept_immediately) else {
            return Ok(Settled::Return(ReadResult::Eof));
        };

        let result = match signal {
            Ok(reedline::Signal::Success(s)) => Ok(ReadResult::Input(s)),
            Ok(reedline::Signal::CtrlC) => Ok(ReadResult::Interrupted),
            Ok(reedline::Signal::CtrlD) => Ok(ReadResult::Eof),
            Ok(reedline::Signal::ExternalBreak(_)) => Err(ShellError::UnexpectedInputFailure),
            Ok(reedline::Signal::HostCommand(cmd)) => Ok(ReadResult::BoundCommand(cmd)),
            Ok(_) => Err(ShellError::UnexpectedInputFailure),
            Err(err) => Err(ShellError::InputError(err)),
        };

        self.settle(result)
    }

    /// Carries out an accept-line a macro asked for mid-body: a second, immediately
    /// accepting read submits the buffer as it stands, and the rest of the macro is left
    /// pending for the read after that.
    fn accept_then_replay(
        &mut self,
        prompt: &InteractivePrompt,
        replay: Option<DeferredReplay>,
    ) -> Result<ReadResult, ShellError> {
        let accepted = self.read_and_settle(prompt, true)?;
        self.pending_replay = replay;
        match accepted {
            Settled::Return(result) => Ok(result),
            // An immediately accepting read dispatches no key, so it cannot settle to
            // anything else; if it does, treat it as an empty line rather than end the
            // session.
            other => {
                tracing::debug!(
                    target: trace_categories::INPUT,
                    "an accepting read settled to {other:?}"
                );
                Ok(ReadResult::Input(String::new()))
            }
        }
    }

    /// Applies macro bytes deferred from before the last bound command, resolving them
    /// against the bindings as they are *now*. Edits land in the editor buffer straight
    /// away; whatever comes after them is left for the read that follows. `None` means
    /// nothing was pending, or nothing came of it, and the next read proceeds as usual.
    fn apply_pending_replay(&mut self) -> Result<Option<Settled>, ShellError> {
        let Some(replay) = self.pending_replay.take() else {
            return Ok(None);
        };
        let Some(reedline) = self.reedline.as_mut() else {
            return Ok(None);
        };

        // Resolving may hit another bound command, in which case the bytes after it are
        // deferred again.
        let event = lock(&self.bindings).resolve_replay(replay);
        let (edits, stop) = plan_replay(event);
        reedline.run_edit_commands(&edits);

        match stop {
            Some(ReplayStop::HostCommand(cmd)) => {
                self.settle(Ok(ReadResult::BoundCommand(cmd))).map(Some)
            }
            Some(ReplayStop::Accept) => Ok(Some(Settled::Accept(None))),
            None => Ok(None),
        }
    }

    /// Runs reedline's line reader, having it accept the buffer immediately when a replayed
    /// macro asked for that. The flag is a builder option, so the editor is briefly moved out
    /// and back around the call; it is never out during the read itself, keeping the
    /// panic-time handling in [`Drop`] intact.
    fn read_line_from_reedline(
        &mut self,
        prompt: &InteractivePrompt,
        accept_immediately: bool,
    ) -> Option<std::io::Result<reedline::Signal>> {
        self.set_immediately_accept(accept_immediately);
        let signal = self
            .reedline
            .as_mut()
            .map(|r| read_line_with_retries(r, prompt));
        if accept_immediately {
            self.set_immediately_accept(false);
        }

        signal
    }

    fn set_immediately_accept(&mut self, value: bool) {
        if let Some(reedline) = self.reedline.take() {
            self.reedline = Some(reedline.with_immediately_accept(value));
        }
    }
}

/// Reads one line, retrying a bounded number of times when reedline fails.
///
/// An error here is almost always transient. The prevalent case: reedline asks the terminal
/// for the cursor position (DSR, `ESC [ 6 n`) before painting a prompt, and again after an
/// external program (a `bind -x` command such as atuin's search UI, fzf, ...) hands the
/// terminal back. crossterm waits a fixed 2s for the reply and then fails; a terminal busy
/// repainting or a multiplexer briefly holding the reply is enough to trip it, and giving up
/// would end the whole interactive session. That failure happens before any input is read,
/// so re-issuing the read is safe; retry a bounded number of times before treating the
/// failure as real. A terminal that never answers therefore fails after
/// `MAX_READ_LINE_ATTEMPTS` x 2s rather than 2s.
///
/// The one known exception: reedline restores the terminal mode *after* computing its
/// result, so if `disable_raw_mode` itself fails, a line that was already submitted is lost
/// and the retry prompts afresh. That is a tcsetattr failure on a tty that just worked; the
/// alternative -- exiting the shell -- loses the same line and everything else with it.
fn read_line_with_retries(
    reedline: &mut reedline::Reedline,
    prompt: &InteractivePrompt,
) -> std::io::Result<reedline::Signal> {
    let mut attempt: u32 = 1;
    loop {
        match reedline.read_line(prompt) {
            Err(err) if attempt < MAX_READ_LINE_ATTEMPTS => {
                attempt += 1;
                tracing::debug!(
                    target: trace_categories::INPUT,
                    "reedline read_line failed; retrying (attempt {attempt}/{MAX_READ_LINE_ATTEMPTS}): {err}"
                );
            }
            result => return result,
        }
    }
}

fn lock(
    bindings: &Arc<Mutex<edit_mode::UpdatableBindings>>,
) -> tokio::sync::MutexGuard<'_, edit_mode::UpdatableBindings> {
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(bindings.lock()))
}

/// What a settled read asks the backend to do.
#[derive(Debug, PartialEq, Eq)]
enum Settled {
    /// Hand this result to the shell.
    Return(ReadResult),
    /// Accept the buffer as it stands, as accept-line would, then replay these bytes on the
    /// read after.
    Accept(Option<DeferredReplay>),
    /// Nothing left to act on; prompt again. Reached only after a bookkeeping bug.
    Resume,
}

/// The event that cut a replay short, on which reedline would have returned to the host.
#[derive(Debug, PartialEq, Eq)]
enum ReplayStop {
    /// An accept-line.
    Accept,
    /// A bound command, as reedline would have returned it, not yet settled.
    HostCommand(String),
}

/// What a host command reedline returned settles to: a command for the shell to run.
fn settled_for(host_command: HostCommand) -> Settled {
    match host_command {
        HostCommand::Command(command) => Settled::Return(ReadResult::BoundCommand(command)),
        HostCommand::Deferred(id) => {
            // A record never names another record; one here is a bookkeeping bug.
            tracing::debug!(
                target: trace_categories::INPUT,
                "deferred record {id} nested in a bound command"
            );
            Settled::Resume
        }
    }
}

/// Splits a resolved macro event into the edits to apply now and the event, if any, that
/// stops the replay there.
///
/// Anything reedline itself would have to drive (menus, history search, completion) can't
/// be replayed from outside its read loop and is dropped; reedline exposes no way to queue
/// events into the next read. Edits and accept-line cover what bound commands leave behind
/// in practice.
fn plan_replay(event: reedline::ReedlineEvent) -> (Vec<reedline::EditCommand>, Option<ReplayStop>) {
    let mut edits = Vec::new();

    for event in events::flatten(event) {
        match event {
            reedline::ReedlineEvent::Edit(commands) => edits.extend(commands),
            reedline::ReedlineEvent::Enter => return (edits, Some(ReplayStop::Accept)),
            reedline::ReedlineEvent::ExecuteHostCommand(cmd) => {
                return (edits, Some(ReplayStop::HostCommand(cmd)));
            }
            other => {
                tracing::debug!(
                    target: brush_core::trace_categories::INPUT,
                    "dropping unsupported deferred macro event: {other:?}"
                );
            }
        }
    }

    (edits, None)
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
        loop {
            let settled = match self.apply_pending_replay()? {
                Some(settled) => settled,
                None => self.read_and_settle(&prompt, false)?,
            };

            match settled {
                Settled::Return(result) => return Ok(result),
                Settled::Accept(then) => return self.accept_then_replay(&prompt, then),
                Settled::Resume => (),
            }
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
                reedline::EditCommand::Clear,
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

    // Add undo. readline binds it to Ctrl+_, which terminals send as 0x1f; crossterm reports
    // that byte as Ctrl+7.
    key_bindings.add_binding(
        reedline::KeyModifiers::CONTROL,
        reedline::KeyCode::Char('7'),
        reedline::ReedlineEvent::Edit(vec![reedline::EditCommand::Undo]),
    );

    // Ctrl+J accepts the line, as in readline. In raw mode a newline byte arrives as Ctrl+J
    // rather than Enter, and macro bodies spell accept-line as `\n` at least as often as
    // `\r`.
    key_bindings.add_binding(
        reedline::KeyModifiers::CONTROL,
        reedline::KeyCode::Char('j'),
        reedline::ReedlineEvent::Enter,
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
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use brush_core::interfaces::{KeyBindings as _, KeyMacro, KeySequence};

    fn control_key(character: char) -> KeySequence {
        KeySequence::from(vec![brush_parser::readline_binding::control_byte(
            character as u8,
        )])
    }

    fn shell_bindings() -> edit_mode::UpdatableBindings {
        edit_mode::UpdatableBindings::new(compose_key_bindings(COMPLETION_MENU_NAME))
    }

    #[test]
    fn every_binding_the_shell_lists_is_one_bind_accepts_back() {
        // `bind -p` names a key's function by translating the event bound to it, and an
        // inputrc built from that listing is fed straight back to `bind`. Any event we can
        // name but not translate back would list a line `bind` then rejects.
        let bindings = compose_key_bindings(COMPLETION_MENU_NAME);
        for (key, event) in bindings.get_keybindings() {
            let Some(action) = events::translate_reedline_event_to_action(event) else {
                continue;
            };
            assert!(
                events::translate_action_to_reedline_event(&action).is_some(),
                "{key:?} lists as `{action}`, which `bind` does not accept"
            );
        }
    }

    #[test]
    fn set_read_buffer_replaces_all_lines() {
        for old_cursor in [0, 6, 14, usize::MAX] {
            for (replacement, cursor) in [("new", 1), ("new\nlines\n", 4), ("", 0)] {
                let mut backend = ReedlineInputBackend {
                    reedline: Some(reedline::Reedline::create()),
                    bindings: Arc::new(Mutex::new(shell_bindings())),
                    pending_replay: None,
                };
                backend.set_read_buffer("first\nsecond\nthird".to_owned(), old_cursor);
                backend.set_read_buffer(replacement.to_owned(), cursor);
                assert_eq!(
                    backend.get_read_buffer(),
                    Some((replacement.to_owned(), cursor)),
                    "old cursor {old_cursor}, replacement {replacement:?}"
                );
            }
        }
    }

    #[test]
    fn set_read_buffer_preserves_byte_cursor_clamping() {
        let replacement = "a\u{e9}\n\u{754c}z";
        for (requested, expected) in [(0, 0), (2, 1), (3, 3), (5, 4), (8, 8), (usize::MAX, 8)] {
            let mut backend = ReedlineInputBackend {
                reedline: Some(reedline::Reedline::create()),
                bindings: Arc::new(Mutex::new(shell_bindings())),
                pending_replay: None,
            };
            backend.set_read_buffer("old\nbuffer".to_owned(), 4);
            backend.set_read_buffer(replacement.to_owned(), requested);
            assert_eq!(
                backend.get_read_buffer(),
                Some((replacement.to_owned(), expected)),
                "requested byte offset {requested}"
            );
        }
    }

    #[test]
    fn control_underscore_macro_undoes() -> Result<(), ShellError> {
        let mut bindings = shell_bindings();
        bindings.define_macro(control_key('g'), KeyMacro::from(b"\x1f".to_vec()))?;

        assert_eq!(
            edit_mode::press(
                &mut bindings,
                reedline::KeyModifiers::CONTROL,
                reedline::KeyCode::Char('g')
            )?,
            reedline::ReedlineEvent::Edit(vec![reedline::EditCommand::Undo])
        );

        Ok(())
    }

    #[test]
    fn tab_in_macro_completes() -> Result<(), ShellError> {
        let mut bindings = shell_bindings();
        bindings.define_macro(control_key('g'), KeyMacro::from(b"ab\t".to_vec()))?;

        let tab = bindings
            .get_current()
            .get(&KeySequence::from(b"\t".to_vec()))
            .cloned();
        assert!(tab.is_some(), "tab should be listed as bound");

        let event = edit_mode::press(
            &mut bindings,
            reedline::KeyModifiers::CONTROL,
            reedline::KeyCode::Char('g'),
        )?;
        let reedline::ReedlineEvent::Multiple(events) = event else {
            return Err(ShellError::IoError(std::io::Error::other(std::format!(
                "expected text followed by the tab binding, got {event:?}"
            ))));
        };
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0],
            reedline::ReedlineEvent::Edit(vec![reedline::EditCommand::InsertString(
                "ab".to_owned()
            )])
        );
        assert!(matches!(events[1], reedline::ReedlineEvent::UntilFound(_)));

        Ok(())
    }

    #[test]
    fn insert_comment_binding_spliced_into_macro() -> Result<(), ShellError> {
        // A base binding whose event is itself a `Multiple` (\M-# inserts a comment and
        // accepts) nests inside the macro's events as-is.
        let mut bindings = shell_bindings();
        bindings.define_macro(control_key('g'), KeyMacro::from(b"ls\x1b#".to_vec()))?;

        assert_eq!(
            edit_mode::press(
                &mut bindings,
                reedline::KeyModifiers::CONTROL,
                reedline::KeyCode::Char('g')
            )?,
            reedline::ReedlineEvent::Multiple(vec![
                reedline::ReedlineEvent::Edit(vec![reedline::EditCommand::InsertString(
                    "ls".to_owned()
                )]),
                reedline::ReedlineEvent::Multiple(vec![
                    reedline::ReedlineEvent::Edit(vec![
                        reedline::EditCommand::MoveToStart { select: false },
                        reedline::EditCommand::InsertChar('#'),
                    ]),
                    reedline::ReedlineEvent::Enter,
                ]),
            ])
        );

        Ok(())
    }

    #[test]
    fn accept_line_nested_in_a_bound_event_defers_the_rest() -> Result<(), ShellError> {
        // \M-# is bound to a `Multiple` that ends in an accept-line. With bytes after it in
        // the macro, resolution stops there like it would at a bare accept-line, keeping the
        // edits before it and deferring what follows; bash runs both lines.
        let mut bindings = shell_bindings();
        bindings.define_macro(
            control_key('g'),
            KeyMacro::from(b"ls\x1b#echo b\r".to_vec()),
        )?;

        let event = edit_mode::press(
            &mut bindings,
            reedline::KeyModifiers::CONTROL,
            reedline::KeyCode::Char('g'),
        )?;
        let reedline::ReedlineEvent::Multiple(events) = &event else {
            return Err(ShellError::IoError(std::io::Error::other(std::format!(
                "expected text, the comment edit and a deferred record, got {event:?}"
            ))));
        };
        assert_eq!(events.len(), 3);
        assert_eq!(
            events[0],
            reedline::ReedlineEvent::Edit(vec![reedline::EditCommand::InsertString(
                "ls".to_owned()
            )])
        );
        assert_eq!(
            events[1],
            reedline::ReedlineEvent::Edit(vec![
                reedline::EditCommand::MoveToStart { select: false },
                reedline::EditCommand::InsertChar('#'),
            ])
        );
        let reedline::ReedlineEvent::ExecuteHostCommand(encoded) = &events[2] else {
            return Err(ShellError::UnexpectedInputFailure);
        };
        let HostCommand::Deferred(id) = HostCommand::decode(encoded) else {
            return Err(ShellError::UnexpectedInputFailure);
        };
        let deferred = bindings
            .end_read(Some(id))
            .ok_or(ShellError::UnexpectedInputFailure)?;
        assert_eq!(deferred.action, events::DeferredAction::AcceptLine);
        let replay = deferred.replay.ok_or(ShellError::UnexpectedInputFailure)?;
        assert_eq!(replay.bytes(), b"echo b\r");

        Ok(())
    }

    #[test]
    fn plan_replay_applies_edits_then_accepts() {
        let (edits, outcome) = plan_replay(reedline::ReedlineEvent::Multiple(vec![
            reedline::ReedlineEvent::Edit(vec![reedline::EditCommand::InsertString(
                "echo".to_owned(),
            )]),
            reedline::ReedlineEvent::Enter,
            reedline::ReedlineEvent::Edit(vec![reedline::EditCommand::InsertString(
                "dropped".to_owned(),
            )]),
        ]));

        assert_eq!(
            edits,
            vec![reedline::EditCommand::InsertString("echo".to_owned())]
        );
        assert_eq!(outcome, Some(ReplayStop::Accept));
    }

    #[test]
    fn plan_replay_stops_at_bound_command() {
        let (edits, outcome) = plan_replay(reedline::ReedlineEvent::Multiple(vec![
            reedline::ReedlineEvent::Edit(vec![reedline::EditCommand::InsertString(
                "pre".to_owned(),
            )]),
            reedline::ReedlineEvent::ExecuteHostCommand("bound".to_owned()),
            reedline::ReedlineEvent::Enter,
        ]));

        assert_eq!(
            edits,
            vec![reedline::EditCommand::InsertString("pre".to_owned())]
        );
        assert_eq!(outcome, Some(ReplayStop::HostCommand("bound".to_owned())));
    }

    #[test]
    fn plan_replay_flattens_nested_events_and_skips_unsupported_ones() {
        let (edits, outcome) = plan_replay(reedline::ReedlineEvent::Multiple(vec![
            reedline::ReedlineEvent::Multiple(vec![reedline::ReedlineEvent::Edit(vec![
                reedline::EditCommand::InsertChar('a'),
            ])]),
            reedline::ReedlineEvent::SearchHistory,
            reedline::ReedlineEvent::None,
            reedline::ReedlineEvent::Edit(vec![reedline::EditCommand::InsertChar('b')]),
        ]));

        assert_eq!(
            edits,
            vec![
                reedline::EditCommand::InsertChar('a'),
                reedline::EditCommand::InsertChar('b'),
            ]
        );
        assert_eq!(outcome, None);
    }

    #[test]
    fn plan_replay_of_nothing_continues() {
        let (edits, outcome) = plan_replay(reedline::ReedlineEvent::None);
        assert!(edits.is_empty());
        assert_eq!(outcome, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_unclaimable_deferred_record_resumes_without_pending_replay() {
        // Nothing produces one: a record is claimed exactly once, by the read that returned
        // its index. If our bookkeeping ever slips, the session must survive it.
        let mut backend = ReedlineInputBackend {
            reedline: None,
            bindings: Arc::new(Mutex::new(shell_bindings())),
            pending_replay: Some(DeferredReplay::new(
                b"stale",
                edit_mode::MAX_MACRO_REPLAY_BYTES,
            )),
        };
        let settled = backend.settle(Ok(ReadResult::BoundCommand(
            HostCommand::Deferred(usize::MAX).encode(),
        )));

        assert_eq!(settled.ok(), Some(Settled::Resume));
        assert!(backend.pending_replay.is_none());
    }

    #[test]
    fn a_record_nested_in_a_bound_command_resumes() {
        // A record's action never names another record; one here would be a bug, not a
        // reason to end the session.
        assert_eq!(settled_for(HostCommand::Deferred(3)), Settled::Resume);
        assert_eq!(
            settled_for(HostCommand::Command("echo hi".to_owned())),
            Settled::Return(ReadResult::BoundCommand("echo hi".to_owned()))
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn pending_replay_edits_the_buffer_and_leaves_accepting_to_reedline()
    -> Result<(), ShellError> {
        let bindings = Arc::new(Mutex::new(shell_bindings()));
        bindings.lock().await.define_macro(
            control_key('g'),
            KeyMacro::from(b"echo replayed\r".to_vec()),
        )?;

        let mut backend = ReedlineInputBackend {
            reedline: Some(reedline::Reedline::create()),
            bindings: bindings.clone(),
            pending_replay: None,
        };

        // Nothing pending: an ordinary read follows.
        assert_eq!(backend.apply_pending_replay()?, None);

        // Pending bytes that trigger a macro: its text lands in the buffer, and the accept
        // is left for reedline to perform on the read that follows.
        backend.pending_replay = Some(DeferredReplay::new(
            b"\x07",
            edit_mode::MAX_MACRO_REPLAY_BYTES,
        ));
        assert_eq!(backend.apply_pending_replay()?, Some(Settled::Accept(None)));
        assert_eq!(
            backend.get_read_buffer(),
            Some(("echo replayed".to_owned(), 13))
        );
        assert!(backend.pending_replay.is_none());

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn pending_replay_returns_bound_command_and_defers_the_rest() -> Result<(), ShellError> {
        let bindings = Arc::new(Mutex::new(shell_bindings()));
        bindings.lock().await.bind(
            control_key('t'),
            brush_core::interfaces::KeyAction::ShellCommand("bound".to_owned()),
        )?;

        let mut backend = ReedlineInputBackend {
            reedline: Some(reedline::Reedline::create()),
            bindings: bindings.clone(),
            pending_replay: Some(DeferredReplay::new(
                b"pre\x14post",
                edit_mode::MAX_MACRO_REPLAY_BYTES,
            )),
        };

        assert_eq!(
            backend.apply_pending_replay()?,
            Some(Settled::Return(ReadResult::BoundCommand(
                "bound".to_owned()
            )))
        );
        assert_eq!(backend.get_read_buffer(), Some(("pre".to_owned(), 3)));

        // The bytes after the command are now pending on the backend.
        assert_eq!(
            backend.pending_replay.as_ref().map(DeferredReplay::bytes),
            Some(b"post".to_vec())
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn pending_replay_without_an_editor_is_dropped() -> Result<(), ShellError> {
        let bindings = Arc::new(Mutex::new(shell_bindings()));
        bindings.lock().await.bind(
            control_key('t'),
            brush_core::interfaces::KeyAction::ShellCommand("bound".to_owned()),
        )?;

        let mut backend = ReedlineInputBackend {
            reedline: None,
            bindings: bindings.clone(),
            pending_replay: Some(DeferredReplay::new(
                b"\x14tail",
                edit_mode::MAX_MACRO_REPLAY_BYTES,
            )),
        };

        assert_eq!(backend.apply_pending_replay()?, None);
        assert!(backend.pending_replay.is_none());

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn read_results_other_than_bound_commands_drop_deferred_bytes() -> Result<(), ShellError>
    {
        let bindings = Arc::new(Mutex::new(shell_bindings()));
        bindings.lock().await.bind(
            control_key('t'),
            brush_core::interfaces::KeyAction::ShellCommand("bound".to_owned()),
        )?;
        bindings
            .lock()
            .await
            .define_macro(control_key('g'), KeyMacro::from(b"\x14tail".to_vec()))?;

        let mut backend = ReedlineInputBackend {
            reedline: Some(reedline::Reedline::create()),
            bindings: bindings.clone(),
            pending_replay: None,
        };

        // A macro press that reedline never turned into a bound command (say, because it
        // was in history-search mode) must not leak its tail into a later read.
        let _ = edit_mode::press(
            &mut *bindings.lock().await,
            reedline::KeyModifiers::CONTROL,
            reedline::KeyCode::Char('g'),
        )?;
        let settled = backend.settle(Ok(ReadResult::Input("typed".to_owned())))?;
        assert!(matches!(settled, Settled::Return(ReadResult::Input(line)) if line == "typed"));
        assert!(backend.pending_replay.is_none());

        // Whereas the record reedline returns for the command maps back to the command
        // itself, and its tail becomes pending.
        let event = edit_mode::press(
            &mut *bindings.lock().await,
            reedline::KeyModifiers::CONTROL,
            reedline::KeyCode::Char('g'),
        )?;
        let reedline::ReedlineEvent::ExecuteHostCommand(encoded) = event else {
            return Err(ShellError::IoError(std::io::Error::other(std::format!(
                "expected a bound command, got {event:?}"
            ))));
        };
        assert_ne!(encoded, "bound");
        let settled = backend.settle(Ok(ReadResult::BoundCommand(encoded)))?;
        assert!(
            matches!(&settled, Settled::Return(ReadResult::BoundCommand(cmd)) if cmd == "bound"),
            "unexpected settled result: {settled:?}"
        );
        assert_eq!(
            backend.pending_replay.as_ref().map(DeferredReplay::bytes),
            Some(b"tail".to_vec())
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn accept_line_mid_macro_accepts_now_and_replays_the_rest_later() -> Result<(), ShellError>
    {
        let bindings = Arc::new(Mutex::new(shell_bindings()));
        bindings.lock().await.define_macro(
            control_key('g'),
            KeyMacro::from(b"echo a\recho b\r".to_vec()),
        )?;

        let mut backend = ReedlineInputBackend {
            reedline: Some(reedline::Reedline::create()),
            bindings: bindings.clone(),
            pending_replay: None,
        };

        let event = edit_mode::press(
            &mut *bindings.lock().await,
            reedline::KeyModifiers::CONTROL,
            reedline::KeyCode::Char('g'),
        )?;
        let reedline::ReedlineEvent::Multiple(events) = event else {
            return Err(ShellError::IoError(std::io::Error::other(std::format!(
                "unexpected event: {event:?}"
            ))));
        };
        let Some(reedline::ReedlineEvent::ExecuteHostCommand(encoded)) = events.last().cloned()
        else {
            return Err(ShellError::IoError(std::io::Error::other(
                "macro did not stop at accept-line",
            )));
        };

        let settled = backend.settle(Ok(ReadResult::BoundCommand(encoded)))?;
        assert!(
            matches!(&settled, Settled::Accept(Some(replay)) if replay.bytes() == b"echo b\r"),
            "unexpected settled result: {settled:?}"
        );

        // The same through a deferred replay: the outcome carries what to replay after the
        // accept, so the accept read can't clear it.
        backend.pending_replay = Some(DeferredReplay::new(
            b"\x07",
            edit_mode::MAX_MACRO_REPLAY_BYTES,
        ));
        let outcome = backend.apply_pending_replay()?;
        assert!(
            matches!(&outcome, Some(Settled::Accept(Some(replay))) if replay.bytes() == b"echo b\r"),
            "unexpected outcome: {outcome:?}"
        );
        assert_eq!(backend.get_read_buffer(), Some(("echo a".to_owned(), 6)));

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_read_still_ends_the_read() -> Result<(), ShellError> {
        let bindings = Arc::new(Mutex::new(shell_bindings()));
        {
            let mut bindings = bindings.lock().await;
            bindings.bind(
                control_key('t'),
                brush_core::interfaces::KeyAction::ShellCommand("bound".to_owned()),
            )?;
            bindings.bind(
                KeySequence::from(b"\x18\x12".to_vec()),
                brush_core::interfaces::KeyAction::ShellCommand("raw".to_owned()),
            )?;

            // Leave a held prefix key behind, as a read that failed midway would.
            let _ = edit_mode::press(
                &mut bindings,
                reedline::KeyModifiers::CONTROL,
                reedline::KeyCode::Char('x'),
            )?;
            drop(bindings);
        }

        let mut backend = ReedlineInputBackend {
            reedline: None,
            bindings: bindings.clone(),
            pending_replay: Some(DeferredReplay::new(
                b"stale",
                edit_mode::MAX_MACRO_REPLAY_BYTES,
            )),
        };

        let settled = backend.settle(Err(ShellError::UnexpectedInputFailure));
        assert!(matches!(settled, Err(ShellError::UnexpectedInputFailure)));
        assert!(backend.pending_replay.is_none());

        let mut bindings = bindings.lock().await;
        let after_prefix = edit_mode::press(
            &mut bindings,
            reedline::KeyModifiers::CONTROL,
            reedline::KeyCode::Char('r'),
        )?;
        drop(bindings);

        assert_ne!(
            after_prefix,
            reedline::ReedlineEvent::ExecuteHostCommand("raw".to_owned())
        );

        Ok(())
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
