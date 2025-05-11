use brush_core::interfaces::{self, InputFunction, Key, KeyAction, KeySequence, KeyStroke};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

#[derive(thiserror::Error, Debug)]
pub enum KeyError {
    /// Unsupported key sequence
    #[error("unsupported key sequence: {0}")]
    UnsupportedKeySequence(KeySequence),

    /// Unsupported key action
    #[error("unsupported key action: {0}")]
    UnsupportedKeyAction(KeyAction),
}

pub(crate) struct MutableEditMode {
    inner: Arc<Mutex<UpdatableBindings>>,
}

impl MutableEditMode {
    pub fn new(bindings: reedline::Keybindings) -> Self {
        Self {
            inner: Arc::new(Mutex::new(UpdatableBindings::new(bindings))),
        }
    }

    pub fn bindings(&self) -> Arc<Mutex<UpdatableBindings>> {
        self.inner.clone()
    }
}

impl reedline::EditMode for MutableEditMode {
    fn parse_event(&mut self, event: reedline::ReedlineRawEvent) -> reedline::ReedlineEvent {
        let mut inner = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.inner.lock())
        });

        inner.parse_event(event)
    }

    fn edit_mode(&self) -> reedline::PromptEditMode {
        let inner = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.inner.lock())
        });

        inner.edit_mode()
    }
}

pub(crate) struct UpdatableBindings {
    bindings: reedline::Keybindings,
    edit_mode: Box<dyn reedline::EditMode>,
}

impl UpdatableBindings {
    pub fn new(bindings: reedline::Keybindings) -> Self {
        // Clone the bindings so we can keep a copy for later updates.
        let edit_mode = Self::rebuild_edit_mode(&bindings);

        Self {
            bindings,
            edit_mode,
        }
    }

    pub fn update(&mut self, f: impl Fn(&mut reedline::Keybindings)) {
        f(&mut self.bindings);
        self.edit_mode = Self::rebuild_edit_mode(&self.bindings);
    }

    fn rebuild_edit_mode(bindings: &reedline::Keybindings) -> Box<dyn reedline::EditMode> {
        Box::new(reedline::Emacs::new(bindings.clone()))
    }
}

impl reedline::EditMode for UpdatableBindings {
    fn parse_event(&mut self, event: reedline::ReedlineRawEvent) -> reedline::ReedlineEvent {
        self.edit_mode.parse_event(event)
    }

    fn edit_mode(&self) -> reedline::PromptEditMode {
        self.edit_mode.edit_mode()
    }
}

impl interfaces::KeyBindings for UpdatableBindings {
    fn get_current(&self) -> HashMap<interfaces::KeySequence, interfaces::KeyAction> {
        let mut results = HashMap::new();

        for (key_combo, event) in self.bindings.get_keybindings() {
            let action = translate_reedline_event_to_action(event);
            if let Some(action) = action {
                if let Some(key) = translate_reedline_keycode(key_combo.key_code) {
                    let mut stroke = KeyStroke::from(key);

                    if key_combo.modifier.contains(reedline::KeyModifiers::CONTROL) {
                        stroke.control = true;
                    }
                    if key_combo.modifier.contains(reedline::KeyModifiers::ALT) {
                        stroke.alt = true;
                    }
                    if key_combo.modifier.contains(reedline::KeyModifiers::SHIFT) {
                        stroke.shift = true;
                    }
                    if key_combo.modifier.contains(reedline::KeyModifiers::HYPER) {
                        // TODO
                    }
                    if key_combo.modifier.contains(reedline::KeyModifiers::META) {
                        // TODO
                    }
                    if key_combo.modifier.contains(reedline::KeyModifiers::SUPER) {
                        // TODO
                    }

                    let seq = KeySequence::from(stroke);
                    results.insert(seq, action);
                }
            }
        }

        results
    }

    fn bind(&mut self, seq: KeySequence, action: KeyAction) -> Result<(), std::io::Error> {
        let Some((modifiers, key_code)) = translate_key_sequence_to_reedline(&seq) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                KeyError::UnsupportedKeySequence(seq),
            ));
        };

        let Some(event) = translate_action_to_reedline_event(&action) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                KeyError::UnsupportedKeyAction(action),
            ));
        };

        self.update(|bindings| {
            bindings.add_binding(modifiers, key_code, event.clone());
        });

        Ok(())
    }
}

fn translate_key_sequence_to_reedline(
    seq: &KeySequence,
) -> Option<(reedline::KeyModifiers, reedline::KeyCode)> {
    if seq.strokes.len() != 1 {
        // TODO: handle multiple strokes
        return None;
    }

    let stroke = &seq.strokes[0];

    let mut modifiers = reedline::KeyModifiers::empty();
    modifiers.set(reedline::KeyModifiers::ALT, stroke.alt);
    modifiers.set(reedline::KeyModifiers::CONTROL, stroke.control);
    modifiers.set(reedline::KeyModifiers::SHIFT, stroke.shift);

    let key_code = match stroke.key {
        Key::Character(c) => reedline::KeyCode::Char(c),
        Key::Backspace => reedline::KeyCode::Backspace,
        Key::Enter => reedline::KeyCode::Enter,
        Key::Left => reedline::KeyCode::Left,
        Key::Right => reedline::KeyCode::Right,
        Key::Up => reedline::KeyCode::Up,
        Key::Down => reedline::KeyCode::Down,
        Key::Home => reedline::KeyCode::Home,
        Key::End => reedline::KeyCode::End,
        Key::PageUp => reedline::KeyCode::PageUp,
        Key::PageDown => reedline::KeyCode::PageDown,
        Key::Tab => reedline::KeyCode::Tab,
        Key::BackTab => reedline::KeyCode::BackTab,
        Key::Delete => reedline::KeyCode::Delete,
        Key::Insert => reedline::KeyCode::Insert,
        Key::F(n) => reedline::KeyCode::F(n),
        Key::Escape => reedline::KeyCode::Esc,
    };

    Some((modifiers, key_code))
}

fn translate_action_to_reedline_event(action: &KeyAction) -> Option<reedline::ReedlineEvent> {
    match action {
        KeyAction::ShellCommand(cmd) => {
            Some(reedline::ReedlineEvent::ExecuteHostCommand(cmd.to_owned()))
        }
        KeyAction::DoInputFunction(_input_function) => None, // TODO: implement
    }
}

fn translate_reedline_keycode(keycode: reedline::KeyCode) -> Option<Key> {
    match keycode {
        reedline::KeyCode::Backspace => Some(Key::Backspace),
        reedline::KeyCode::Enter => Some(Key::Enter),
        reedline::KeyCode::Left => Some(Key::Left),
        reedline::KeyCode::Right => Some(Key::Right),
        reedline::KeyCode::Up => Some(Key::Up),
        reedline::KeyCode::Down => Some(Key::Down),
        reedline::KeyCode::Home => Some(Key::Home),
        reedline::KeyCode::End => Some(Key::End),
        reedline::KeyCode::PageUp => Some(Key::PageUp),
        reedline::KeyCode::PageDown => Some(Key::PageDown),
        reedline::KeyCode::Tab => Some(Key::Tab),
        reedline::KeyCode::BackTab => Some(Key::BackTab),
        reedline::KeyCode::Delete => Some(Key::Delete),
        reedline::KeyCode::Insert => Some(Key::Insert),
        reedline::KeyCode::F(n) => Some(Key::F(n)),
        reedline::KeyCode::Char(c) => Some(Key::Character(c)),
        reedline::KeyCode::Null => None,
        reedline::KeyCode::Esc => Some(Key::Escape),
        reedline::KeyCode::CapsLock => None,
        reedline::KeyCode::ScrollLock => None,
        reedline::KeyCode::NumLock => None,
        reedline::KeyCode::PrintScreen => None,
        reedline::KeyCode::Pause => None,
        reedline::KeyCode::Menu => None,
        reedline::KeyCode::KeypadBegin => None,
        reedline::KeyCode::Media(_media_key_code) => None,
        reedline::KeyCode::Modifier(_modifier_key_code) => None,
    }
}

#[allow(clippy::too_many_lines)]
fn translate_reedline_event_to_action(event: &reedline::ReedlineEvent) -> Option<KeyAction> {
    match event {
        reedline::ReedlineEvent::Edit(cmds) => {
            match cmds.as_slice() {
                [reedline::EditCommand::Backspace] => Some(KeyAction::DoInputFunction(
                    InputFunction::BackwardDeleteChar,
                )),
                [reedline::EditCommand::BackspaceWord] => {
                    // Not quite accurate, because it doesn't save the deleted text.
                    Some(KeyAction::DoInputFunction(InputFunction::BackwardKillWord))
                }
                [reedline::EditCommand::CapitalizeChar] => None,
                [reedline::EditCommand::ClearToLineEnd] => {
                    // Not quite accurate, because it doesn't save the deleted text.
                    Some(KeyAction::DoInputFunction(InputFunction::KillLine))
                }
                [reedline::EditCommand::CutFromStart] => {
                    Some(KeyAction::DoInputFunction(InputFunction::KillWholeLine))
                }
                [reedline::EditCommand::CutToLineEnd] => {
                    Some(KeyAction::DoInputFunction(InputFunction::KillLine))
                }
                [reedline::EditCommand::CutWordLeft] => {
                    Some(KeyAction::DoInputFunction(InputFunction::BackwardKillWord))
                }
                [reedline::EditCommand::CutWordRight] => {
                    Some(KeyAction::DoInputFunction(InputFunction::KillWord))
                }
                [reedline::EditCommand::Delete] => {
                    Some(KeyAction::DoInputFunction(InputFunction::DeleteChar))
                }
                [reedline::EditCommand::DeleteWord] => {
                    Some(KeyAction::DoInputFunction(InputFunction::KillWord))
                }
                [reedline::EditCommand::InsertNewline] => None,
                [reedline::EditCommand::LowercaseWord] => {
                    Some(KeyAction::DoInputFunction(InputFunction::DowncaseWord))
                }
                [reedline::EditCommand::MoveLeft { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::BackwardChar))
                }
                [reedline::EditCommand::MoveLeft { select: true }] => None,
                [reedline::EditCommand::MoveRight { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::ForwardChar))
                }
                [reedline::EditCommand::MoveRight { select: true }] => None,
                [reedline::EditCommand::MoveToEnd { select: false }] => {
                    // TODO: Not quite accurate, because it doesn't just go to end of line.
                    Some(KeyAction::DoInputFunction(InputFunction::EndOfLine))
                }
                [reedline::EditCommand::MoveToEnd { select: true }] => None,
                [reedline::EditCommand::MoveToLineEnd { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::EndOfLine))
                }
                [reedline::EditCommand::MoveToLineEnd { select: true }] => None,
                [reedline::EditCommand::MoveToLineStart { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::BeginningOfLine))
                }
                [reedline::EditCommand::MoveToLineStart { select: true }] => None,
                [reedline::EditCommand::MoveToStart { select: false }] => {
                    // TODO: Not quite accurate, because it doesn't just go to beginning of line.
                    Some(KeyAction::DoInputFunction(InputFunction::BeginningOfLine))
                }
                [reedline::EditCommand::MoveToStart { select: true }] => None,
                [reedline::EditCommand::MoveWordLeft { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::BackwardWord))
                }
                [reedline::EditCommand::MoveWordLeft { select: true }] => None,
                [reedline::EditCommand::MoveWordRight { select: false }] => {
                    Some(KeyAction::DoInputFunction(InputFunction::ForwardWord))
                }
                [reedline::EditCommand::MoveWordRight { select: true }] => None,
                [reedline::EditCommand::PasteCutBufferAfter] => {
                    Some(KeyAction::DoInputFunction(InputFunction::Yank))
                }
                [reedline::EditCommand::PasteCutBufferBefore] => None,
                [reedline::EditCommand::Redo] => {
                    Some(KeyAction::DoInputFunction(InputFunction::ViRedo))
                }
                [reedline::EditCommand::SelectAll] => None,
                [reedline::EditCommand::SwapGraphemes] => {
                    Some(KeyAction::DoInputFunction(InputFunction::TransposeChars))
                }
                [reedline::EditCommand::UppercaseWord] => {
                    Some(KeyAction::DoInputFunction(InputFunction::UpcaseWord))
                }
                [reedline::EditCommand::Undo] => {
                    Some(KeyAction::DoInputFunction(InputFunction::Undo))
                }
                _ => {
                    // TODO: Handle more?
                    tracing::warn!("unhandled edit commands: {cmds:?}");
                    None
                }
            }
        }
        reedline::ReedlineEvent::ClearScreen => {
            Some(KeyAction::DoInputFunction(InputFunction::ClearScreen))
        }
        reedline::ReedlineEvent::CtrlC => None,
        reedline::ReedlineEvent::CtrlD => None,
        reedline::ReedlineEvent::Enter => {
            Some(KeyAction::DoInputFunction(InputFunction::AcceptLine))
        }
        reedline::ReedlineEvent::Esc => None,
        reedline::ReedlineEvent::MenuPrevious => None,
        reedline::ReedlineEvent::OpenEditor => None,
        reedline::ReedlineEvent::SearchHistory => Some(KeyAction::DoInputFunction(
            InputFunction::HistorySearchBackward,
        )),
        reedline::ReedlineEvent::Multiple(_) => {
            // TODO: Try to extract something from these?
            None
        }
        reedline::ReedlineEvent::UntilFound(uf_events) => {
            if let [reedline::ReedlineEvent::HistoryHintComplete
            | reedline::ReedlineEvent::HistoryHintWordComplete, next_evt] = uf_events.as_slice()
            {
                translate_reedline_event_to_action(next_evt)
            } else {
                // TODO: Try to extract something from these?
                None
            }
        }
        _ => {
            // TODO: Handle more?
            None
        }
    }
}
