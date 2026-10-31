//! Shell-facing binding configuration. Reedline owns all live modal state.

use std::collections::HashMap;

use brush_core::interfaces::{
    InputFunction, Key, KeyAction, KeyBindings, KeyMap, KeySequence, KeyStroke,
};
use reedline::{
    EditCommand, EditMode, KeyCode, KeyModifiers, Keybindings, PromptEditMode, PromptViMode,
    ReedlineEvent,
};

type BindingId = (KeyModifiers, KeyCode);
type EditorPair = (Box<dyn EditMode>, Box<dyn EditMode>);

const MAX_MACRO_BYTES: usize = 4096;
const MAX_COMPILED_COMMAND_BYTES: usize = 64 * 1024;
const MAX_MACRO_WORK: usize = 16 * 1024;
const MAX_ALIAS_DEPTH: usize = 32;

#[derive(Default)]
struct MacroExpansion {
    events: Vec<ReedlineEvent>,
    command_bytes: usize,
    work: usize,
}

enum IntermediateTarget<'a> {
    Action(&'a KeyAction),
    Alias(&'a KeySequence, &'a [Vec<u8>]),
}

struct KeyMapState {
    bindings: Keybindings,
    actions: HashMap<KeySequence, KeyAction>,
    macros: HashMap<KeySequence, KeySequence>,
    /// Intermediate Emacs sequences resolved during configuration, not terminal input.
    raw_mappings: HashMap<Vec<u8>, KeyAction>,
}

impl KeyMapState {
    fn new(bindings: Keybindings) -> Self {
        let actions = bindings
            .get_keybindings()
            .iter()
            .filter_map(|(key, event)| {
                Some((
                    binding_sequence((key.modifier, key.key_code))?,
                    event_action(event)?,
                ))
            })
            .collect();
        Self {
            bindings,
            actions,
            macros: HashMap::new(),
            raw_mappings: HashMap::new(),
        }
    }

    /// Restore eager longest-prefix translation, including bounded acyclic aliases.
    /// This only compiles configuration; no keys are replayed through an editor.
    fn resolve_emacs_macro(&self, bytes: &[u8]) -> Result<ReedlineEvent, std::io::Error> {
        let mut expansion = MacroExpansion::default();
        self.append_emacs_macro(bytes, &mut expansion, &mut Vec::new())?;
        Ok(match expansion.events.len() {
            0 => ReedlineEvent::None,
            1 => expansion.events.remove(0),
            _ => ReedlineEvent::Multiple(expansion.events),
        })
    }

    fn append_emacs_macro<'a>(
        &'a self,
        bytes: &[u8],
        expansion: &mut MacroExpansion,
        active: &mut Vec<&'a KeySequence>,
    ) -> Result<(), std::io::Error> {
        expansion.work = expansion.work.saturating_add(bytes.len());
        if expansion.work > MAX_MACRO_WORK {
            return Err(unsupported("macro exceeds compilation work limit"));
        }
        let mut remaining = bytes;
        while !remaining.is_empty() {
            let actions = self
                .raw_mappings
                .iter()
                .map(|(key, action)| (key.as_slice(), IntermediateTarget::Action(action)));
            let aliases = self.macros.iter().filter_map(|(seq, target)| {
                let KeySequence::Bytes(parts) = seq else {
                    return None;
                };
                let [key] = parts.as_slice() else { return None };
                let body = match target {
                    KeySequence::Bytes(parts) => parts.as_slice(),
                    KeySequence::Strokes(strokes) if strokes.is_empty() => &[],
                    KeySequence::Strokes(_) => return None,
                };
                Some((key.as_slice(), IntermediateTarget::Alias(seq, body)))
            });
            let matched = actions
                .chain(aliases)
                .filter(|(key, _)| remaining.starts_with(key))
                .max_by_key(|(key, _)| key.len());
            if let Some((key, target)) = matched {
                match target {
                    IntermediateTarget::Action(action) => {
                        if let KeyAction::ShellCommand(command) = action {
                            expansion.command_bytes =
                                expansion.command_bytes.saturating_add(command.len());
                        }
                        if expansion.command_bytes > MAX_COMPILED_COMMAND_BYTES
                            || expansion.events.len() >= MAX_MACRO_BYTES
                        {
                            return Err(unsupported("macro exceeds compiled output limit"));
                        }
                        expansion.events.push(
                            action_event(action)
                                .ok_or_else(|| unsupported("unsupported intermediate action"))?,
                        );
                    }
                    IntermediateTarget::Alias(seq, body) => {
                        if active.len() >= MAX_ALIAS_DEPTH || active.contains(&seq) {
                            return Err(unsupported(
                                "cyclic or excessively deep intermediate macro",
                            ));
                        }
                        active.push(seq);
                        self.append_emacs_macro(&body.concat(), expansion, active)?;
                        active.pop();
                    }
                }
                remaining = &remaining[key.len()..];
            } else {
                remaining = &remaining[1..];
            }
        }
        Ok(())
    }

    /// Compile before touching live tables so a rejected intermediate replacement
    /// leaves every previously compiled trigger intact.
    fn update_raw_mapping(
        &mut self,
        bytes: Vec<u8>,
        action: Option<KeyAction>,
        macro_target: Option<KeySequence>,
    ) -> Result<(), std::io::Error> {
        let seq = KeySequence::Bytes(vec![bytes.clone()]);
        let previous_macro = if let Some(target) = macro_target {
            self.macros.insert(seq.clone(), target)
        } else {
            self.macros.remove(&seq)
        };
        let previous = if let Some(action) = action {
            self.raw_mappings.insert(bytes.clone(), action)
        } else {
            self.raw_mappings.remove(&bytes)
        };
        let compiled = self
            .macros
            .iter()
            .filter_map(|(seq, target)| {
                let KeySequence::Bytes(parts) = target else {
                    return None;
                };
                Some(
                    self.resolve_emacs_macro(&parts.concat())
                        .map(|event| binding_id(seq).map(|id| (id, event))),
                )
            })
            .collect::<Result<Vec<_>, _>>();
        match compiled {
            Ok(compiled) => {
                for ((modifiers, code), event) in compiled.into_iter().flatten() {
                    self.bindings.add_binding(modifiers, code, event);
                }
                Ok(())
            }
            Err(error) => {
                if let Some(previous_macro) = previous_macro {
                    self.macros.insert(seq, previous_macro);
                } else {
                    self.macros.remove(&seq);
                }
                if let Some(previous) = previous {
                    self.raw_mappings.insert(bytes, previous);
                } else {
                    self.raw_mappings.remove(&bytes);
                }
                Err(error)
            }
        }
    }
}

/// Configuration shared with `bind`, never installed as a Reedline edit mode.
pub(crate) struct UpdatableBindings {
    emacs: KeyMapState,
    vi_insert: KeyMapState,
    vi_command: KeyMapState,
    current_keymap: KeyMap,
    revision: u64,
}

impl UpdatableBindings {
    pub fn new(emacs: Keybindings, vi_insert: Keybindings, vi_command: Keybindings) -> Self {
        Self {
            emacs: KeyMapState::new(emacs),
            vi_insert: KeyMapState::new(vi_insert),
            vi_command: KeyMapState::new(vi_command),
            // Readline's initial binding map before the first interactive read.
            current_keymap: KeyMap::EmacsStandard,
            revision: 0,
        }
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub const fn set_current_mode(&mut self, mode: &PromptEditMode) {
        self.current_keymap = match mode {
            PromptEditMode::Vi(PromptViMode::Normal | PromptViMode::Visual) => KeyMap::ViCommand,
            PromptEditMode::Vi(PromptViMode::Insert) => KeyMap::ViInsert,
            _ => KeyMap::EmacsStandard,
        };
    }

    /// Build the two library machines, preserving the active prompt state when bindings change.
    pub fn editors(&self, active: PromptEditMode) -> EditorPair {
        let emacs = Box::new(reedline::Emacs::new(self.emacs.bindings.clone()));
        let mut vi = reedline::Vi::new(
            self.vi_insert.bindings.clone(),
            self.vi_command.bindings.clone(),
            reedline::default_vi_visual_keybindings(),
        );
        if matches!(active, PromptEditMode::Vi(_)) {
            // This initializes a replacement machine in the same state, not a live transition.
            // EventStatus is private in Reedline 0.52; wrappers cannot implement this hook.
            let _ = vi.handle_mode_specific_event(ReedlineEvent::SwitchMode(active));
            (Box::new(vi), emacs)
        } else {
            (emacs, Box::new(vi))
        }
    }

    const fn state(&self, map: KeyMap) -> Option<&KeyMapState> {
        match map {
            KeyMap::EmacsStandard | KeyMap::EmacsMeta => Some(&self.emacs),
            KeyMap::ViInsert => Some(&self.vi_insert),
            KeyMap::ViCommand => Some(&self.vi_command),
            KeyMap::EmacsCtlx => None,
        }
    }

    const fn state_mut(&mut self, map: KeyMap) -> Option<&mut KeyMapState> {
        match map {
            KeyMap::EmacsStandard | KeyMap::EmacsMeta => Some(&mut self.emacs),
            KeyMap::ViInsert => Some(&mut self.vi_insert),
            KeyMap::ViCommand => Some(&mut self.vi_command),
            KeyMap::EmacsCtlx => None,
        }
    }

    fn validate_binding(
        &self,
        map: KeyMap,
        seq: &KeySequence,
    ) -> Result<BindingId, std::io::Error> {
        if !self.supports_keymap(map) {
            return Err(unsupported(
                "Reedline does not support the emacs-ctlx prefix map",
            ));
        }
        let mut id =
            binding_id(seq).ok_or_else(|| unsupported("Reedline requires a single decoded key"))?;
        if map == KeyMap::EmacsMeta {
            if id.0.contains(KeyModifiers::ALT) {
                return Err(unsupported(
                    "nested meta prefixes require multi-key input replay",
                ));
            }
            id.0.insert(KeyModifiers::ALT);
        }
        // A custom binding cannot change a motion inside a pending operator in 0.52.
        // Reject that promise rather than stealing the key from the vi parser.
        if (map == KeyMap::ViCommand && is_vi_motion(id))
            || (matches!(map, KeyMap::ViInsert | KeyMap::ViCommand)
                && id == (KeyModifiers::NONE, KeyCode::Esc))
        {
            return Err(unsupported(
                "rebinding vi grammar motions requires Reedline support",
            ));
        }
        if matches!(map, KeyMap::EmacsStandard | KeyMap::ViInsert) {
            if let Some(state) = self.state(map) {
                if state.macros.values().any(|target| match target {
                    KeySequence::Strokes(strokes) => strokes
                        .iter()
                        .any(|stroke| binding_id(&KeySequence::from(stroke.clone())) == Some(id)),
                    KeySequence::Bytes(_) => false,
                }) {
                    return Err(unsupported(
                        "key is used by a literal macro; remove the macro first",
                    ));
                }
            }
        }
        Ok(id)
    }
}

impl KeyBindings for UpdatableBindings {
    fn supports_keymap(&self, map: KeyMap) -> bool {
        self.state(map).is_some()
    }

    fn get_current(&self) -> HashMap<KeySequence, KeyAction> {
        self.get_current_for(self.current_keymap)
    }

    fn get_current_for(&self, map: KeyMap) -> HashMap<KeySequence, KeyAction> {
        self.state(map)
            .map(|state| {
                state
                    .actions
                    .iter()
                    .filter_map(|(seq, action)| Some((view_sequence(map, seq)?, action.clone())))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_untranslated(&self, bytes: &[u8]) -> Option<&KeyAction> {
        self.state(self.current_keymap)?.raw_mappings.get(bytes)
    }

    fn bind(&mut self, seq: KeySequence, action: KeyAction) -> Result<(), std::io::Error> {
        self.bind_for(self.current_keymap, seq, action)
    }

    fn bind_for(
        &mut self,
        map: KeyMap,
        seq: KeySequence,
        action: KeyAction,
    ) -> Result<(), std::io::Error> {
        if let Some(bytes) = emacs_raw_sequence(map, &seq)? {
            // Validate even unresolved actions before retaining them for macros.
            if action_event(&action).is_none() {
                return Err(unsupported("Readline action is not supported by Reedline"));
            }
            self.emacs.update_raw_mapping(bytes, Some(action), None)?;
            self.revision = self.revision.wrapping_add(1);
            return Ok(());
        }
        let (modifiers, code) = self.validate_binding(map, &seq)?;
        let event = action_event(&action)
            .ok_or_else(|| unsupported("Readline action is not supported by Reedline"))?;
        let seq =
            binding_sequence((modifiers, code)).ok_or_else(|| unsupported("unsupported key"))?;
        let state = self
            .state_mut(map)
            .ok_or_else(|| unsupported("unsupported keymap"))?;
        state.macros.remove(&seq);
        state.bindings.add_binding(modifiers, code, event);
        state.actions.insert(seq, action);
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    fn try_unbind(&mut self, seq: KeySequence) -> bool {
        self.try_unbind_for(self.current_keymap, seq)
    }

    fn try_unbind_for(&mut self, map: KeyMap, seq: KeySequence) -> bool {
        match emacs_raw_sequence(map, &seq) {
            Ok(Some(bytes)) => {
                let seq = KeySequence::Bytes(vec![bytes.clone()]);
                let removed = self.emacs.raw_mappings.contains_key(&bytes)
                    || self.emacs.macros.contains_key(&seq);
                if removed && self.emacs.update_raw_mapping(bytes, None, None).is_ok() {
                    self.revision = self.revision.wrapping_add(1);
                    return true;
                }
                return false;
            }
            Err(_) => return false,
            Ok(None) => {}
        }
        let Ok((modifiers, code)) = self.validate_binding(map, &seq) else {
            return false;
        };
        let Some(seq) = binding_sequence((modifiers, code)) else {
            return false;
        };
        let Some(state) = self.state_mut(map) else {
            return false;
        };
        let removed = state.macros.contains_key(&seq)
            || state
                .bindings
                .find_binding(modifiers, code)
                .is_some_and(|event| event != ReedlineEvent::None);
        state.actions.remove(&seq);
        state.macros.remove(&seq);
        if removed {
            state
                .bindings
                .add_binding(modifiers, code, ReedlineEvent::None);
            self.revision = self.revision.wrapping_add(1);
        }
        removed
    }

    fn define_macro(
        &mut self,
        seq: KeySequence,
        target: KeySequence,
    ) -> Result<(), std::io::Error> {
        self.define_macro_for(self.current_keymap, seq, target)
    }

    fn define_macro_for(
        &mut self,
        map: KeyMap,
        seq: KeySequence,
        target: KeySequence,
    ) -> Result<(), std::io::Error> {
        if let Some(bytes) = emacs_raw_sequence(map, &seq)? {
            match &target {
                KeySequence::Strokes(strokes) if strokes.is_empty() => {}
                KeySequence::Bytes(parts) => validate_macro_bytes(parts)?,
                KeySequence::Strokes(_) => {
                    return Err(unsupported("intermediate macro requires a byte body"));
                }
            }
            self.emacs.update_raw_mapping(bytes, None, Some(target))?;
            self.revision = self.revision.wrapping_add(1);
            return Ok(());
        }
        let (modifiers, code) = self.validate_binding(map, &seq)?;
        if map == KeyMap::ViCommand
            || (matches!(code, KeyCode::Char(_))
                && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT))
        {
            return Err(unsupported(
                "modal or recursive macros require ordered Reedline input replay",
            ));
        }
        if let KeySequence::Bytes(parts) = &target {
            validate_macro_bytes(parts)?;
            if matches!(map, KeyMap::EmacsStandard | KeyMap::EmacsMeta)
                && parts.iter().flatten().any(u8::is_ascii_control)
            {
                let event = self.emacs.resolve_emacs_macro(&parts.concat())?;
                let seq = binding_sequence((modifiers, code))
                    .ok_or_else(|| unsupported("unsupported key"))?;
                self.emacs.bindings.add_binding(modifiers, code, event);
                self.emacs.actions.remove(&seq);
                self.emacs.macros.insert(seq, target);
                self.revision = self.revision.wrapping_add(1);
                return Ok(());
            }
        }
        // The bind parser may deliver an ordinary string as a byte chunk rather
        // than individual strokes. Decode only bounded UTF-8 text, never keys or
        // terminal escape sequences. This still compiles to a single insertion.
        let target = match target {
            KeySequence::Strokes(_) => target,
            KeySequence::Bytes(bytes) => {
                let text = String::from_utf8(bytes.concat())
                    .map_err(|_| unsupported("macro must contain literal UTF-8 text"))?;
                KeySequence::Strokes(
                    text.chars()
                        .map(|c| KeyStroke::from(Key::Character(c)))
                        .collect(),
                )
            }
        };
        let KeySequence::Strokes(strokes) = &target else {
            unreachable!()
        };
        let mut text = String::new();
        let insert_state = if map == KeyMap::ViInsert {
            &self.vi_insert
        } else {
            &self.emacs
        };
        for stroke in strokes {
            let Key::Character(c) = stroke.key else {
                return Err(unsupported("only literal insertion macros are supported"));
            };
            if stroke.alt || stroke.control || c.is_control() {
                return Err(unsupported(
                    "control-key macros require ordered Reedline input replay",
                ));
            }
            let (mods, key) = binding_id(&KeySequence::from(stroke.clone()))
                .ok_or_else(|| unsupported("unsupported macro key"))?;
            if insert_state.bindings.find_binding(mods, key).is_some() {
                return Err(unsupported(
                    "macro refers to a bound key; ordered input replay is required",
                ));
            }
            text.push(if stroke.shift {
                c.to_ascii_uppercase()
            } else {
                c
            });
            if text.len() > MAX_MACRO_BYTES {
                return Err(unsupported("literal macro exceeds 4096 bytes"));
            }
        }
        let seq =
            binding_sequence((modifiers, code)).ok_or_else(|| unsupported("unsupported key"))?;
        let state = self
            .state_mut(map)
            .ok_or_else(|| unsupported("unsupported keymap"))?;
        state.bindings.add_binding(
            modifiers,
            code,
            ReedlineEvent::Edit(vec![EditCommand::InsertString(text)]),
        );
        state.actions.remove(&seq);
        state.macros.insert(seq, target);
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    fn get_macros(&self) -> HashMap<KeySequence, KeySequence> {
        self.get_macros_for(self.current_keymap)
    }

    fn get_macros_for(&self, map: KeyMap) -> HashMap<KeySequence, KeySequence> {
        self.state(map)
            .map(|state| {
                state
                    .macros
                    .iter()
                    .filter_map(|(seq, target)| Some((view_sequence(map, seq)?, target.clone())))
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn unsupported(message: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Unsupported, message)
}

fn validate_macro_bytes(parts: &[Vec<u8>]) -> Result<(), std::io::Error> {
    if parts
        .iter()
        .try_fold(0usize, |len, part| len.checked_add(part.len()))
        .is_none_or(|len| len > MAX_MACRO_BYTES)
    {
        return Err(unsupported("macro exceeds 4096 bytes"));
    }
    Ok(())
}

fn emacs_raw_sequence(map: KeyMap, seq: &KeySequence) -> Result<Option<Vec<u8>>, std::io::Error> {
    let KeySequence::Bytes(parts) = seq else {
        return Ok(None);
    };
    if !matches!(map, KeyMap::EmacsStandard | KeyMap::EmacsMeta) {
        return Err(unsupported(
            "raw intermediate bindings require an Emacs map",
        ));
    }
    validate_macro_bytes(parts)?;
    let mut bytes = parts.concat();
    if bytes.is_empty() {
        return Err(unsupported("empty intermediate key sequence"));
    }
    if map == KeyMap::EmacsMeta {
        bytes.insert(0, b'\x1b');
        if bytes.len() > MAX_MACRO_BYTES {
            return Err(unsupported("macro exceeds 4096 bytes"));
        }
    }
    Ok(Some(bytes))
}

const fn is_vi_motion((modifiers, code): BindingId) -> bool {
    matches!(modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT)
        && matches!(
            code,
            KeyCode::Char(
                'h' | 'j' | 'k' | 'l' | 'w' | 'b' | 'e' | 'f' | 't' | 'g' | 'v' | '0'
                    ..='9' | '^' | '$' | '%' | ';' | ','
            )
        )
}

/// The meta map is a view of the standard map's Alt chords, not a second
/// table that could shadow later bindings or resurrect unbound keys.
fn view_sequence(map: KeyMap, seq: &KeySequence) -> Option<KeySequence> {
    if map != KeyMap::EmacsMeta {
        return Some(seq.clone());
    }
    let (mut modifiers, code) = binding_id(seq)?;
    if !modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    modifiers.remove(KeyModifiers::ALT);
    binding_sequence((modifiers, code))
}

fn binding_id(seq: &KeySequence) -> Option<BindingId> {
    let KeySequence::Strokes(strokes) = seq else {
        return None;
    };
    let [stroke] = strokes.as_slice() else {
        return None;
    };
    let mut modifiers = KeyModifiers::empty();
    modifiers.set(KeyModifiers::CONTROL, stroke.control);
    modifiers.set(KeyModifiers::ALT, stroke.alt);
    modifiers.set(KeyModifiers::SHIFT, stroke.shift);
    let code = match stroke.key {
        Key::Character(c) => {
            if c.is_ascii_uppercase() {
                modifiers.insert(KeyModifiers::SHIFT);
                KeyCode::Char(c.to_ascii_lowercase())
            } else {
                KeyCode::Char(c)
            }
        }
        Key::Backspace => KeyCode::Backspace,
        Key::Enter => KeyCode::Enter,
        Key::Left => KeyCode::Left,
        Key::Right => KeyCode::Right,
        Key::Up => KeyCode::Up,
        Key::Down => KeyCode::Down,
        Key::Home => KeyCode::Home,
        Key::End => KeyCode::End,
        Key::PageUp => KeyCode::PageUp,
        Key::PageDown => KeyCode::PageDown,
        Key::Tab => KeyCode::Tab,
        Key::BackTab => KeyCode::BackTab,
        Key::Delete => KeyCode::Delete,
        Key::Insert => KeyCode::Insert,
        Key::F(n) => KeyCode::F(n),
        Key::Escape => KeyCode::Esc,
    };
    Some((modifiers, code))
}

fn binding_sequence((modifiers, code): BindingId) -> Option<KeySequence> {
    let key = match code {
        KeyCode::Char(c) => Key::Character(if modifiers.contains(KeyModifiers::SHIFT) {
            c.to_ascii_uppercase()
        } else {
            c
        }),
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter => Key::Enter,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Delete => Key::Delete,
        KeyCode::Insert => Key::Insert,
        KeyCode::F(n) => Key::F(n),
        KeyCode::Esc => Key::Escape,
        _ => return None,
    };
    Some(KeySequence::from(KeyStroke {
        control: modifiers.contains(KeyModifiers::CONTROL),
        alt: modifiers.contains(KeyModifiers::ALT),
        shift: !matches!(key, Key::Character(_)) && modifiers.contains(KeyModifiers::SHIFT),
        key,
    }))
}

fn action_event(action: &KeyAction) -> Option<ReedlineEvent> {
    match action {
        KeyAction::ShellCommand(command) => {
            Some(ReedlineEvent::ExecuteHostCommand(command.clone()))
        }
        // Multiple stops at suspension/exit events; it is not a resumable
        // ordered host-action sequence. Do not promise replay through barriers.
        KeyAction::Sequence(_) => None,
        KeyAction::DoInputFunction(function) => function_event(function),
    }
}

fn function_event(function: &InputFunction) -> Option<ReedlineEvent> {
    use InputFunction as F;
    let command = match function {
        F::BackwardDeleteChar => EditCommand::Backspace,
        F::BackwardKillWord => EditCommand::CutWordLeft,
        F::UnixLineDiscard => EditCommand::CutFromStart,
        F::KillLine => EditCommand::KillLine,
        F::KillWord => EditCommand::CutWordRight,
        F::DeleteChar => EditCommand::Delete,
        F::DowncaseWord => EditCommand::LowercaseWord,
        F::BackwardChar => EditCommand::MoveLeft { select: false },
        F::ForwardChar => EditCommand::MoveRight { select: false },
        F::EndOfLine => EditCommand::MoveToLineEnd { select: false },
        F::BeginningOfLine => EditCommand::MoveToLineStart { select: false },
        F::BackwardWord => EditCommand::MoveWordLeft { select: false },
        F::ForwardWord => EditCommand::MoveWordRight { select: false },
        F::Yank => EditCommand::PasteCutBufferAfter,
        F::TransposeChars => EditCommand::SwapGraphemes,
        F::UpcaseWord => EditCommand::UppercaseWord,
        F::Undo => EditCommand::Undo,
        F::Complete => EditCommand::Complete,
        F::ClearScreen => return Some(ReedlineEvent::ClearScreen),
        F::AcceptLine => return Some(ReedlineEvent::Enter),
        F::PreviousHistory => return Some(ReedlineEvent::PreviousHistory),
        F::NextHistory => return Some(ReedlineEvent::NextHistory),
        F::ReverseSearchHistory => return Some(ReedlineEvent::SearchHistory),
        F::RedrawCurrentLine => return Some(ReedlineEvent::Repaint),
        F::EmacsEditingMode => return Some(ReedlineEvent::SwitchMode(PromptEditMode::Emacs)),
        F::ViEditingMode => {
            return Some(ReedlineEvent::SwitchMode(PromptEditMode::Vi(
                PromptViMode::Insert,
            )));
        }
        F::BrushAcceptHint => return Some(ReedlineEvent::HistoryHintComplete),
        F::BrushAcceptHintWord => return Some(ReedlineEvent::HistoryHintWordComplete),
        F::ViEditAndExecuteCommand => {
            return Some(ReedlineEvent::Multiple(vec![
                ReedlineEvent::OpenEditor,
                ReedlineEvent::Submit,
            ]));
        }
        _ => return None,
    };
    Some(ReedlineEvent::Edit(vec![command]))
}

fn event_action(event: &ReedlineEvent) -> Option<KeyAction> {
    use InputFunction as F;
    let function = match event {
        ReedlineEvent::ClearScreen => F::ClearScreen,
        ReedlineEvent::Enter => F::AcceptLine,
        ReedlineEvent::PreviousHistory => F::PreviousHistory,
        ReedlineEvent::NextHistory => F::NextHistory,
        ReedlineEvent::SearchHistory => F::ReverseSearchHistory,
        ReedlineEvent::Repaint => F::RedrawCurrentLine,
        ReedlineEvent::ExecuteHostCommand(command) => {
            return Some(KeyAction::ShellCommand(command.clone()));
        }
        ReedlineEvent::Edit(commands) => match commands.as_slice() {
            [EditCommand::Backspace] => F::BackwardDeleteChar,
            [EditCommand::CutWordLeft] => F::BackwardKillWord,
            [EditCommand::CutFromStart] => F::UnixLineDiscard,
            [EditCommand::KillLine] => F::KillLine,
            [EditCommand::CutWordRight] => F::KillWord,
            [EditCommand::Delete] => F::DeleteChar,
            [EditCommand::MoveLeft { select: false }] => F::BackwardChar,
            [EditCommand::MoveRight { select: false }] => F::ForwardChar,
            [EditCommand::MoveToLineEnd { select: false }] => F::EndOfLine,
            [EditCommand::MoveToLineStart { select: false }] => F::BeginningOfLine,
            [EditCommand::Complete] => F::Complete,
            [EditCommand::Undo] => F::Undo,
            _ => return None,
        },
        _ => return None,
    };
    Some(KeyAction::DoInputFunction(function))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{Event, KeyEvent};

    fn bindings() -> UpdatableBindings {
        UpdatableBindings::new(
            reedline::default_emacs_keybindings(),
            reedline::default_vi_insert_keybindings(),
            reedline::default_vi_normal_keybindings(),
        )
    }
    fn key(c: char) -> KeySequence {
        KeySequence::from(KeyStroke::from(Key::Character(c)))
    }
    fn ctrl(c: char) -> KeySequence {
        KeySequence::from(KeyStroke {
            control: true,
            ..KeyStroke::from(Key::Character(c))
        })
    }
    fn raw(c: char, modifiers: KeyModifiers) -> reedline::ReedlineRawEvent {
        reedline::ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(KeyCode::Char(c), modifiers)))
            .unwrap()
    }

    #[test]
    fn emacs_prefix_macro_tracks_late_binding_replacement_and_removal() {
        let mut bindings = bindings();
        let prefix = KeySequence::Bytes(vec![b"\x18\x1fprobe\x07".to_vec()]);
        let target = KeySequence::Bytes(vec![
            b"\x18\x1fprobe\x07".to_vec(),
            b"\x18\x1fchain\x07".to_vec(),
        ]);
        bindings
            .bind_for(
                KeyMap::ViInsert,
                ctrl('r'),
                KeyAction::ShellCommand("vi".into()),
            )
            .unwrap();
        bindings
            .define_macro_for(KeyMap::EmacsStandard, ctrl('r'), target.clone())
            .unwrap();
        let (mut emacs, _) = bindings.editors(PromptEditMode::Emacs);
        assert_eq!(
            emacs.parse_event(raw('r', KeyModifiers::CONTROL)),
            ReedlineEvent::None
        );

        for command in ["first", "replacement"] {
            bindings
                .bind_for(
                    KeyMap::EmacsStandard,
                    prefix.clone(),
                    KeyAction::ShellCommand(command.into()),
                )
                .unwrap();
            let (mut emacs, mut vi) = bindings.editors(PromptEditMode::Emacs);
            assert_eq!(
                emacs.parse_event(raw('r', KeyModifiers::CONTROL)),
                ReedlineEvent::ExecuteHostCommand(command.into())
            );
            assert_eq!(
                vi.parse_event(raw('r', KeyModifiers::CONTROL)),
                ReedlineEvent::ExecuteHostCommand("vi".into())
            );
        }
        assert!(bindings.try_unbind_for(KeyMap::EmacsStandard, prefix));
        let (mut emacs, _) = bindings.editors(PromptEditMode::Emacs);
        assert_eq!(
            emacs.parse_event(raw('r', KeyModifiers::CONTROL)),
            ReedlineEvent::None
        );
        assert_eq!(
            bindings
                .get_macros_for(KeyMap::EmacsStandard)
                .get(&ctrl('r')),
            Some(&target)
        );
    }

    #[test]
    fn emacs_prefix_macro_uses_longest_matches_and_preserves_action_order() {
        let mut bindings = bindings();
        for (bytes, command) in [
            (b"\x18".as_slice(), "short"),
            (b"\x18\x1f".as_slice(), "long"),
            (b"\x07".as_slice(), "last"),
        ] {
            bindings
                .bind_for(
                    KeyMap::EmacsStandard,
                    KeySequence::Bytes(vec![bytes.to_vec()]),
                    KeyAction::ShellCommand(command.into()),
                )
                .unwrap();
        }
        bindings
            .define_macro_for(
                KeyMap::EmacsStandard,
                ctrl('r'),
                KeySequence::Bytes(vec![b"\x18\x1funbound\x07".to_vec()]),
            )
            .unwrap();
        let (mut emacs, _) = bindings.editors(PromptEditMode::Emacs);
        assert_eq!(
            emacs.parse_event(raw('r', KeyModifiers::CONTROL)),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::ExecuteHostCommand("long".into()),
                ReedlineEvent::ExecuteHostCommand("last".into()),
            ])
        );
    }

    #[test]
    fn emacs_prefix_macro_rejects_oversized_replacements_atomically() {
        let mut bindings = bindings();
        let target = KeySequence::Bytes(vec![b"\x18\x1fprobe\x07".to_vec()]);
        bindings
            .define_macro_for(KeyMap::EmacsStandard, ctrl('r'), target.clone())
            .unwrap();
        assert!(
            bindings
                .define_macro_for(
                    KeyMap::EmacsStandard,
                    ctrl('r'),
                    KeySequence::Bytes(vec![vec![b'\x18'; 4097]]),
                )
                .is_err()
        );
        assert_eq!(
            bindings
                .get_macros_for(KeyMap::EmacsStandard)
                .get(&ctrl('r')),
            Some(&target)
        );
    }

    #[test]
    fn emacs_prefix_empty_raw_macro_replaces_and_removes_an_intermediate_action() {
        let mut bindings = bindings();
        let prefix = KeySequence::Bytes(vec![b"\x18\x1fprobe\x07".to_vec()]);
        bindings
            .bind_for(
                KeyMap::EmacsStandard,
                prefix.clone(),
                KeyAction::ShellCommand("probe".into()),
            )
            .unwrap();
        bindings
            .define_macro_for(KeyMap::EmacsStandard, ctrl('r'), prefix.clone())
            .unwrap();
        bindings
            .define_macro_for(
                KeyMap::EmacsStandard,
                prefix.clone(),
                KeySequence::Strokes(vec![]),
            )
            .unwrap();
        assert!(bindings.get_untranslated(b"\x18\x1fprobe\x07").is_none());
        let (mut emacs, _) = bindings.editors(PromptEditMode::Emacs);
        assert_eq!(
            emacs.parse_event(raw('r', KeyModifiers::CONTROL)),
            ReedlineEvent::None
        );
        assert!(bindings.try_unbind_for(KeyMap::EmacsStandard, prefix));
        assert!(bindings.try_unbind_for(KeyMap::EmacsStandard, ctrl('r')));
    }

    #[test]
    fn emacs_prefix_rejects_excessive_compiled_output_without_replacing_target() {
        let mut bindings = bindings();
        let prefix = KeySequence::Bytes(vec![vec![b'\x18']]);
        bindings
            .bind_for(
                KeyMap::EmacsStandard,
                prefix.clone(),
                KeyAction::ShellCommand("probe".into()),
            )
            .unwrap();
        bindings
            .define_macro_for(
                KeyMap::EmacsStandard,
                ctrl('r'),
                KeySequence::Bytes(vec![vec![b'\x18'; 4096]]),
            )
            .unwrap();
        let revision = bindings.revision();
        assert!(
            bindings
                .bind_for(
                    KeyMap::EmacsStandard,
                    prefix,
                    KeyAction::ShellCommand("x".repeat(4096))
                )
                .is_err()
        );
        assert_eq!(bindings.revision(), revision);
        assert_eq!(
            bindings.get_untranslated(b"\x18"),
            Some(&KeyAction::ShellCommand("probe".into()))
        );
        let (mut emacs, _) = bindings.editors(PromptEditMode::Emacs);
        let event = emacs.parse_event(raw('r', KeyModifiers::CONTROL));
        assert!(matches!(event, ReedlineEvent::Multiple(_)));
        let ReedlineEvent::Multiple(events) = event else {
            return;
        };
        assert_eq!(events.len(), 4096);
        assert!(
            events
                .iter()
                .all(|event| *event == ReedlineEvent::ExecuteHostCommand("probe".into()))
        );
    }

    #[test]
    fn emacs_prefix_raw_bindings_do_not_enable_vi_macro_replay() {
        let mut bindings = bindings();
        let prefix = KeySequence::Bytes(vec![b"\x18\x1fprobe\x07".to_vec()]);
        bindings
            .bind_for(
                KeyMap::EmacsStandard,
                prefix.clone(),
                KeyAction::ShellCommand("probe".into()),
            )
            .unwrap();
        for map in [KeyMap::ViInsert, KeyMap::ViCommand, KeyMap::EmacsCtlx] {
            assert!(
                bindings
                    .bind_for(
                        map,
                        prefix.clone(),
                        KeyAction::ShellCommand("wrong map".into())
                    )
                    .is_err()
            );
            assert!(
                bindings
                    .define_macro_for(map, ctrl('r'), prefix.clone())
                    .is_err()
            );
            assert!(!bindings.try_unbind_for(map, prefix.clone()));
        }
        assert_eq!(
            bindings.get_untranslated(b"\x18\x1fprobe\x07"),
            Some(&KeyAction::ShellCommand("probe".into()))
        );
    }

    #[test]
    fn emacs_prefix_alias_tracks_late_sink_replacement_and_removal() {
        let mut bindings = bindings();
        let alias = KeySequence::Bytes(vec![b"\x18alias\x07".to_vec()]);
        let sink = KeySequence::Bytes(vec![b"\x18sink\x07".to_vec()]);
        bindings
            .define_macro_for(KeyMap::EmacsStandard, alias.clone(), sink.clone())
            .unwrap();
        bindings
            .define_macro_for(KeyMap::EmacsStandard, ctrl('r'), alias.clone())
            .unwrap();
        for command in ["first", "replacement"] {
            bindings
                .bind_for(
                    KeyMap::EmacsStandard,
                    sink.clone(),
                    KeyAction::ShellCommand(command.into()),
                )
                .unwrap();
            let (mut emacs, _) = bindings.editors(PromptEditMode::Emacs);
            assert_eq!(
                emacs.parse_event(raw('r', KeyModifiers::CONTROL)),
                ReedlineEvent::ExecuteHostCommand(command.into())
            );
        }
        let revision = bindings.revision();
        assert!(
            bindings
                .define_macro_for(KeyMap::EmacsStandard, sink.clone(), alias.clone())
                .is_err()
        );
        assert_eq!(bindings.revision(), revision);
        assert_eq!(
            bindings.get_untranslated(b"\x18sink\x07"),
            Some(&KeyAction::ShellCommand("replacement".into()))
        );
        assert!(bindings.try_unbind_for(KeyMap::EmacsStandard, sink));
        let (mut emacs, _) = bindings.editors(PromptEditMode::Emacs);
        assert_eq!(
            emacs.parse_event(raw('r', KeyModifiers::CONTROL)),
            ReedlineEvent::None
        );
        assert!(bindings.try_unbind_for(KeyMap::EmacsStandard, alias));
    }

    #[test]
    fn emacs_prefix_alias_expansion_is_bounded_even_without_output() {
        let mut bindings = bindings();
        let mut previous = KeySequence::Bytes(vec![b"\x18unbound\x07".to_vec()]);
        let mut rejected = false;
        for n in 0..32 {
            let alias = KeySequence::Bytes(vec![format!("\x18alias{n}\x07").into_bytes()]);
            let KeySequence::Bytes(parts) = &previous else {
                unreachable!()
            };
            let mut repeated = parts.concat();
            repeated.extend_from_within(..);
            if bindings
                .define_macro_for(
                    KeyMap::EmacsStandard,
                    alias.clone(),
                    KeySequence::Bytes(vec![repeated]),
                )
                .is_err()
            {
                assert!(n >= 2, "small acyclic aliases must be accepted");
                rejected = true;
                assert!(
                    !bindings
                        .get_macros_for(KeyMap::EmacsStandard)
                        .contains_key(&alias)
                );
                break;
            }
            previous = alias;
        }
        assert!(rejected, "branching aliases must have a total-work limit");
    }

    #[test]
    fn initial_unqualified_query_uses_emacs() {
        let mut emacs = Keybindings::new();
        emacs.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char('l'),
            ReedlineEvent::ClearScreen,
        );
        let bindings = UpdatableBindings::new(emacs, Keybindings::new(), Keybindings::new());

        assert_eq!(
            bindings.get_current().get(&ctrl('l')),
            Some(&KeyAction::DoInputFunction(InputFunction::ClearScreen))
        );
    }

    #[test]
    fn initial_unqualified_binding_dispatches_in_emacs() {
        let mut bindings = bindings();
        bindings
            .bind(ctrl('t'), KeyAction::ShellCommand("startup".into()))
            .unwrap();
        bindings
            .bind_for(
                KeyMap::ViInsert,
                ctrl('t'),
                KeyAction::ShellCommand("vi".into()),
            )
            .unwrap();

        let (mut emacs, mut vi) = bindings.editors(PromptEditMode::Emacs);
        assert_eq!(
            emacs.parse_event(raw('t', KeyModifiers::CONTROL)),
            ReedlineEvent::ExecuteHostCommand("startup".into())
        );
        assert_eq!(
            vi.parse_event(raw('t', KeyModifiers::CONTROL)),
            ReedlineEvent::ExecuteHostCommand("vi".into())
        );
    }

    #[test]
    fn later_mode_selection_controls_unqualified_bindings() {
        let mut bindings = bindings();
        for (mode, map) in [
            (PromptEditMode::Vi(PromptViMode::Insert), KeyMap::ViInsert),
            (PromptEditMode::Vi(PromptViMode::Normal), KeyMap::ViCommand),
            (PromptEditMode::Emacs, KeyMap::EmacsStandard),
        ] {
            bindings.set_current_mode(&mode);
            bindings
                .bind(ctrl('t'), KeyAction::ShellCommand("live".into()))
                .unwrap();
            assert_eq!(
                bindings.get_current_for(map).get(&ctrl('t')),
                Some(&KeyAction::ShellCommand("live".into()))
            );
            let (mut editor, _) = bindings.editors(mode);
            assert_eq!(
                editor.parse_event(raw('t', KeyModifiers::CONTROL)),
                ReedlineEvent::ExecuteHostCommand("live".into())
            );
        }
    }

    #[test]
    fn mode_switch_is_applied_only_by_the_engine_hook() {
        let mut bindings = bindings();
        bindings
            .bind_for(
                KeyMap::EmacsStandard,
                ctrl('t'),
                KeyAction::DoInputFunction(InputFunction::ViEditingMode),
            )
            .unwrap();
        let (mut emacs, _) = bindings.editors(PromptEditMode::Emacs);
        assert_eq!(
            emacs.parse_event(raw('t', KeyModifiers::CONTROL)),
            ReedlineEvent::SwitchMode(PromptEditMode::Vi(PromptViMode::Insert))
        );
        assert_eq!(emacs.edit_mode(), PromptEditMode::Emacs);
    }

    #[test]
    fn unsupported_actions_and_rebound_motions_are_rejected() {
        let mut bindings = bindings();
        let before = bindings.get_current_for(KeyMap::ViCommand);
        for (keyseq, function) in [
            (key('q'), InputFunction::ViDelete),
            (key('q'), InputFunction::ViRedo),
            (key('w'), InputFunction::ForwardChar),
        ] {
            assert!(
                bindings
                    .bind_for(
                        KeyMap::ViCommand,
                        keyseq,
                        KeyAction::DoInputFunction(function)
                    )
                    .is_err()
            );
        }
        assert_eq!(bindings.get_current_for(KeyMap::ViCommand), before);
        assert!(
            bindings
                .bind_for(
                    KeyMap::EmacsStandard,
                    ctrl('t'),
                    KeyAction::Sequence(vec![
                        KeyAction::ShellCommand("barrier".into()),
                        KeyAction::DoInputFunction(InputFunction::ForwardChar),
                    ])
                )
                .is_err()
        );
    }

    #[test]
    fn modal_macro_replay_is_rejected_without_installing_a_macro() {
        let mut bindings = bindings();
        assert!(
            bindings
                .define_macro_for(
                    KeyMap::ViCommand,
                    key('x'),
                    KeySequence::Strokes(vec![Key::Character('x').into(); 2])
                )
                .is_err()
        );
        assert!(bindings.get_macros_for(KeyMap::ViCommand).is_empty());
    }

    #[test]
    fn emacs_meta_dispatches_through_library_bindings() {
        let mut bindings = bindings();
        bindings
            .bind_for(
                KeyMap::EmacsMeta,
                key('q'),
                KeyAction::ShellCommand("meta".into()),
            )
            .unwrap();
        let (mut emacs, _) = bindings.editors(PromptEditMode::Emacs);
        assert_eq!(
            emacs.parse_event(raw('q', KeyModifiers::ALT)),
            ReedlineEvent::ExecuteHostCommand("meta".into())
        );
        assert!(
            bindings
                .bind_for(
                    KeyMap::EmacsCtlx,
                    key('q'),
                    KeyAction::ShellCommand("ctlx".into())
                )
                .is_err()
        );
    }

    #[test]
    fn meta_and_standard_alt_bindings_are_the_same_binding() {
        let mut bindings = bindings();
        let alt_q = KeySequence::from(KeyStroke {
            alt: true,
            ..KeyStroke::from(Key::Character('q'))
        });
        bindings
            .bind_for(
                KeyMap::EmacsMeta,
                key('q'),
                KeyAction::ShellCommand("old".into()),
            )
            .unwrap();
        bindings
            .bind_for(
                KeyMap::EmacsStandard,
                alt_q.clone(),
                KeyAction::ShellCommand("new".into()),
            )
            .unwrap();
        let (mut emacs, _) = bindings.editors(PromptEditMode::Emacs);
        assert_eq!(
            emacs.parse_event(raw('q', KeyModifiers::ALT)),
            ReedlineEvent::ExecuteHostCommand("new".into())
        );
        assert_eq!(
            bindings.get_current_for(KeyMap::EmacsMeta).get(&key('q')),
            Some(&KeyAction::ShellCommand("new".into()))
        );
        assert!(bindings.try_unbind_for(KeyMap::EmacsStandard, alt_q));
        assert!(
            !bindings
                .get_current_for(KeyMap::EmacsMeta)
                .contains_key(&key('q'))
        );
    }

    #[test]
    fn equivalent_key_spellings_replace_and_unbind_the_same_binding() {
        let mut bindings = bindings();
        let shifted_x = KeySequence::from(KeyStroke {
            shift: true,
            ..KeyStroke::from(Key::Character('x'))
        });
        bindings
            .bind_for(
                KeyMap::EmacsStandard,
                key('X'),
                KeyAction::ShellCommand("old".into()),
            )
            .unwrap();
        bindings
            .bind_for(
                KeyMap::EmacsStandard,
                shifted_x,
                KeyAction::ShellCommand("new".into()),
            )
            .unwrap();
        assert_eq!(
            bindings
                .get_current_for(KeyMap::EmacsStandard)
                .get(&key('X')),
            Some(&KeyAction::ShellCommand("new".into()))
        );
        assert!(bindings.try_unbind_for(KeyMap::EmacsStandard, key('X')));
        let (mut emacs, _) = bindings.editors(PromptEditMode::Emacs);
        assert_eq!(
            emacs.parse_event(raw('X', KeyModifiers::SHIFT)),
            ReedlineEvent::None
        );
    }

    #[test]
    fn macros_reject_printable_triggers_even_with_shift() {
        let mut bindings = bindings();
        assert!(
            bindings
                .define_macro_for(KeyMap::EmacsStandard, key('X'), key('X'))
                .is_err()
        );
    }

    #[test]
    fn literal_macros_insert_replace_and_unbind_without_replay() {
        let mut bindings = bindings();
        let literal =
            KeySequence::Strokes("Hi!".chars().map(|c| Key::Character(c).into()).collect());
        bindings
            .define_macro_for(KeyMap::EmacsStandard, ctrl('t'), literal.clone())
            .unwrap();
        let (mut emacs, _) = bindings.editors(PromptEditMode::Emacs);
        assert_eq!(
            emacs.parse_event(raw('t', KeyModifiers::CONTROL)),
            ReedlineEvent::Edit(vec![EditCommand::InsertString("Hi!".into())])
        );
        assert!(
            bindings
                .bind_for(
                    KeyMap::EmacsStandard,
                    key('H'),
                    KeyAction::ShellCommand("no".into())
                )
                .is_err()
        );
        bindings
            .bind_for(
                KeyMap::EmacsStandard,
                ctrl('t'),
                KeyAction::ShellCommand("replacement".into()),
            )
            .unwrap();
        assert!(bindings.get_macros_for(KeyMap::EmacsStandard).is_empty());
        bindings
            .define_macro_for(KeyMap::EmacsStandard, ctrl('t'), literal)
            .unwrap();
        assert!(
            !bindings
                .get_current_for(KeyMap::EmacsStandard)
                .contains_key(&ctrl('t'))
        );
        assert!(bindings.try_unbind_for(KeyMap::EmacsStandard, ctrl('t')));
        assert!(bindings.get_macros_for(KeyMap::EmacsStandard).is_empty());
    }

    #[test]
    fn invalid_macro_replacement_is_atomic_and_byte_bounded() {
        let mut bindings = bindings();
        bindings
            .define_macro_for(KeyMap::EmacsStandard, ctrl('t'), key('a'))
            .unwrap();
        for target in [
            ctrl('b'),
            KeySequence::Bytes(vec![vec![0xff]]),
            KeySequence::Strokes(vec![Key::Character('é').into(); 2049]),
        ] {
            assert!(
                bindings
                    .define_macro_for(KeyMap::EmacsStandard, ctrl('t'), target)
                    .is_err()
            );
            assert_eq!(
                bindings
                    .get_macros_for(KeyMap::EmacsStandard)
                    .get(&ctrl('t')),
                Some(&key('a'))
            );
        }
        bindings
            .define_macro_for(
                KeyMap::EmacsStandard,
                ctrl('t'),
                KeySequence::Strokes(vec![Key::Character('é').into(); 2048]),
            )
            .unwrap();
    }

    #[test]
    fn intercepted_vi_keys_and_all_grammar_motion_prefixes_are_rejected() {
        let mut bindings = bindings();
        for seq in [
            key('j'),
            key('k'),
            key('g'),
            key('G'),
            key('2'),
            key('v'),
            KeySequence::from(KeyStroke::from(Key::Escape)),
        ] {
            assert!(
                bindings
                    .bind_for(KeyMap::ViCommand, seq, KeyAction::ShellCommand("no".into()))
                    .is_err()
            );
        }
        assert!(
            bindings
                .bind_for(
                    KeyMap::ViInsert,
                    KeySequence::from(KeyStroke::from(Key::Escape)),
                    KeyAction::ShellCommand("no".into())
                )
                .is_err()
        );
    }
}
