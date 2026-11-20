//! Key bindings: readline's Emacs-mode defaults, and those made with `bind`.

use std::collections::HashMap;

use brush_core::interfaces::{self, InputFunction, Key, KeyAction, KeySequence, KeyStroke};
use indexmap::IndexMap;

use super::keys::{self, Keypress};

/// The key bindings of a [`super::VtInputBackend`]: keys (or sequences of keys)
/// bound to readline functions, to shell commands (`bind -x`), and to macros
/// (keys to process as if typed). They start as readline's Emacs-mode defaults,
/// which `bind` can change like any others.
pub(crate) struct VtKeyBindings {
    /// What each sequence is bound to, in the order bound. Where more than one
    /// matches the keys pressed (e.g., Tab, and Ctrl-I, which sends the same
    /// byte), the latest wins.
    targets: IndexMap<KeySequence, Target>,
}

/// What a sequence is bound to, as `bind` has it.
enum Target {
    Action(KeyAction),
    Macro(KeySequence),
}

/// What a sequence of keys does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Binding {
    /// A readline function to perform.
    Function(InputFunction),
    /// A shell command to run.
    Command(String),
    /// Keys to process as if typed, as the bytes the terminal would send.
    Macro(Vec<u8>),
}

/// What the keys pressed so far amount to.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Lookup {
    /// What they're bound to, if anything.
    pub bound: Option<Binding>,
    /// Whether a longer bound sequence starts with them.
    pub prefix: bool,
}

impl Default for VtKeyBindings {
    fn default() -> Self {
        let mut bindings = Self {
            targets: IndexMap::new(),
        };
        for (stroke, function) in default_bindings() {
            bindings.set(
                KeySequence::Strokes(vec![stroke]),
                Target::Action(KeyAction::DoInputFunction(function)),
            );
        }
        bindings
    }
}

impl VtKeyBindings {
    /// Looks up what `keys` are bound to.
    pub fn lookup(&self, keys: &[Keypress]) -> Lookup {
        let mut lookup = Lookup::default();
        for (seq, target) in &self.targets {
            // Sequences of actions aren't supported.
            if matches!(target, Target::Action(KeyAction::Sequence(_))) {
                continue;
            }
            match compare(seq, keys) {
                Comparison::Equal => lookup.bound = binding_of(target),
                Comparison::Prefix => lookup.prefix = true,
                Comparison::Different => {}
            }
        }
        lookup
    }

    /// Binds `seq` to `target`, replacing what it was bound to; it's then the
    /// latest binding.
    fn set(&mut self, seq: KeySequence, target: Target) {
        self.remove(&seq);
        self.targets.insert(seq, target);
    }

    /// Removes the binding of `seq`, and of any other way of writing it (e.g.,
    /// `\C-m` for Enter); returns whether there was one.
    fn remove(&mut self, seq: &KeySequence) -> bool {
        let bytes = only_bytes(seq);
        let count = self.targets.len();
        self.targets
            .retain(|other, _| other != seq && (bytes.is_none() || only_bytes(other) != bytes));
        self.targets.len() != count
    }
}

fn binding_of(target: &Target) -> Option<Binding> {
    match target {
        Target::Action(KeyAction::DoInputFunction(function)) => {
            Some(Binding::Function(function.clone()))
        }
        Target::Action(KeyAction::ShellCommand(command)) => Some(Binding::Command(command.clone())),
        Target::Action(KeyAction::Sequence(_)) => None,
        Target::Macro(keys) => Some(Binding::Macro(sequence_bytes(keys))),
    }
}

/// Readline's Emacs-mode bindings, of the functions this backend performs.
fn default_bindings() -> Vec<(KeyStroke, InputFunction)> {
    use InputFunction as F;

    let plain = KeyStroke::from;
    let ctrl = |c| KeyStroke {
        control: true,
        ..Key::Character(c).into()
    };
    let alt = |c| KeyStroke {
        alt: true,
        ..Key::Character(c).into()
    };
    let ctrl_key = |key| KeyStroke {
        control: true,
        ..KeyStroke::from(key)
    };
    let alt_key = |key| KeyStroke {
        alt: true,
        ..KeyStroke::from(key)
    };

    vec![
        (plain(Key::Enter), F::AcceptLine),
        (ctrl('j'), F::AcceptLine),
        (ctrl('a'), F::BeginningOfLine),
        (plain(Key::Home), F::BeginningOfLine),
        (ctrl('e'), F::EndOfLine),
        (plain(Key::End), F::EndOfLine),
        (ctrl('b'), F::BackwardChar),
        (plain(Key::Left), F::BackwardChar),
        (ctrl('f'), F::ForwardChar),
        (plain(Key::Right), F::ForwardChar),
        (alt('b'), F::BackwardWord),
        (ctrl_key(Key::Left), F::BackwardWord),
        (alt_key(Key::Left), F::BackwardWord),
        (alt('f'), F::ForwardWord),
        (ctrl_key(Key::Right), F::ForwardWord),
        (alt_key(Key::Right), F::ForwardWord),
        (plain(Key::Backspace), F::BackwardDeleteChar),
        (ctrl('h'), F::BackwardDeleteChar),
        (ctrl('d'), F::DeleteChar),
        (plain(Key::Delete), F::DeleteChar),
        (alt_key(Key::Backspace), F::BackwardKillWord),
        (
            KeyStroke {
                alt: true,
                ..ctrl('h')
            },
            F::BackwardKillWord,
        ),
        (alt('d'), F::KillWord),
        (ctrl('k'), F::KillLine),
        (ctrl('u'), F::UnixLineDiscard),
        (ctrl('w'), F::UnixWordRubout),
        (ctrl('y'), F::Yank),
        (ctrl('t'), F::TransposeChars),
        (ctrl('p'), F::PreviousHistory),
        (plain(Key::Up), F::PreviousHistory),
        (ctrl('n'), F::NextHistory),
        (plain(Key::Down), F::NextHistory),
        (alt('<'), F::BeginningOfHistory),
        (alt('>'), F::EndOfHistory),
        (ctrl('r'), F::ReverseSearchHistory),
        (ctrl('l'), F::ClearScreen),
        (plain(Key::Tab), F::Complete),
        (ctrl('g'), F::Abort),
    ]
}

#[derive(Debug, PartialEq, Eq)]
enum Comparison {
    Equal,
    /// The keys are the start of the sequence.
    Prefix,
    Different,
}

/// Compares a bound sequence with the keys pressed.
fn compare(seq: &KeySequence, keys: &[Keypress]) -> Comparison {
    match seq {
        // A sequence of keys matches key by key.
        KeySequence::Strokes(strokes) => {
            let matches = keys.len() <= strokes.len()
                && keys
                    .iter()
                    .zip(strokes)
                    .all(|(key, stroke)| is_stroke(key, stroke));
            if !matches {
                Comparison::Different
            } else if keys.len() == strokes.len() {
                Comparison::Equal
            } else {
                Comparison::Prefix
            }
        }
        // `bind` records a sequence it can't interpret as keys as the bytes the
        // terminal sends for it (not necessarily grouped by key), so match those as
        // a whole.
        KeySequence::Bytes(bytes) => {
            let expected = bytes.concat();
            let typed: Vec<u8> = keys
                .iter()
                .flat_map(|key| key.bytes.iter().copied())
                .collect();
            if typed == expected {
                Comparison::Equal
            } else if !typed.is_empty() && expected.starts_with(&typed) {
                Comparison::Prefix
            } else {
                Comparison::Different
            }
        }
    }
}

/// Whether `key` is `stroke`: the same key, or one the terminal sends the same
/// bytes for (e.g., Tab for Ctrl-I, or Backspace for Ctrl-?).
fn is_stroke(key: &Keypress, stroke: &KeyStroke) -> bool {
    key.stroke.as_ref() == Some(stroke)
        || (!key.bytes.is_empty() && key.bytes == keys::encode(stroke))
}

/// Returns the bytes `seq` matches, if there's only one way to send it: as bytes,
/// or keys that each send a single byte (e.g., Enter, as CR). A key that can be
/// sent several ways (e.g., Up, as `\e[A` or `\eOA`) has no such bytes.
fn only_bytes(seq: &KeySequence) -> Option<Vec<u8>> {
    match seq {
        KeySequence::Bytes(bytes) => Some(bytes.concat()),
        KeySequence::Strokes(strokes) => strokes
            .iter()
            .map(|stroke| match keys::encode(stroke).as_slice() {
                [byte] => Some(*byte),
                _ => None,
            })
            .collect(),
    }
}

/// Returns the bytes a terminal would send for the keys in `seq`.
fn sequence_bytes(seq: &KeySequence) -> Vec<u8> {
    match seq {
        KeySequence::Bytes(bytes) => bytes.concat(),
        KeySequence::Strokes(strokes) => strokes.iter().flat_map(keys::encode).collect(),
    }
}

impl interfaces::KeyBindings for VtKeyBindings {
    fn get_current(&self) -> HashMap<KeySequence, KeyAction> {
        self.targets
            .iter()
            .filter_map(|(seq, target)| match target {
                Target::Action(action) => Some((seq.clone(), action.clone())),
                Target::Macro(_) => None,
            })
            .collect()
    }

    fn get_untranslated(&self, bytes: &[u8]) -> Option<&KeyAction> {
        match self.targets.get(&KeySequence::Bytes(vec![bytes.to_vec()])) {
            Some(Target::Action(action)) => Some(action),
            _ => None,
        }
    }

    fn bind(&mut self, seq: KeySequence, action: KeyAction) -> Result<(), std::io::Error> {
        self.set(seq, Target::Action(action));
        Ok(())
    }

    fn try_unbind(&mut self, seq: KeySequence) -> bool {
        self.remove(&seq)
    }

    fn define_macro(
        &mut self,
        seq: KeySequence,
        target: KeySequence,
    ) -> Result<(), std::io::Error> {
        self.set(seq, Target::Macro(target));
        Ok(())
    }

    fn get_macros(&self) -> HashMap<KeySequence, KeySequence> {
        self.targets
            .iter()
            .filter_map(|(seq, target)| match target {
                Target::Macro(keys) => Some((seq.clone(), keys.clone())),
                Target::Action(_) => None,
            })
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn, reason = "assertions in tests")]
mod tests {
    use super::*;
    use interfaces::KeyBindings as _;

    fn ctrl(c: char) -> KeyStroke {
        KeyStroke {
            control: true,
            ..Key::Character(c).into()
        }
    }

    /// Returns the keys a terminal sends as `bytes`.
    fn typed(bytes: &[u8]) -> Vec<Keypress> {
        keys::decode_all(bytes)
            .into_iter()
            .map(|key| Keypress {
                from_macro: false,
                ..key
            })
            .collect()
    }

    fn command(command: &str) -> KeyAction {
        KeyAction::ShellCommand(command.into())
    }

    fn bound_command(command: &str) -> Lookup {
        Lookup {
            bound: Some(Binding::Command(command.into())),
            prefix: false,
        }
    }

    fn unbound() -> Lookup {
        Lookup::default()
    }

    #[test]
    fn starts_with_readline_emacs_bindings() {
        let bindings = VtKeyBindings::default();
        assert_eq!(
            bindings.lookup(&typed(b"\x01")).bound,
            Some(Binding::Function(InputFunction::BeginningOfLine))
        );
        assert_eq!(
            bindings.lookup(&typed(b"\x1b[1;5D")).bound,
            Some(Binding::Function(InputFunction::BackwardWord))
        );
        // `bind` sees them as it sees any other bindings.
        assert_eq!(
            bindings
                .get_current()
                .get(&KeySequence::Strokes(vec![ctrl('r')])),
            Some(&KeyAction::DoInputFunction(
                InputFunction::ReverseSearchHistory
            ))
        );
        // A plain character isn't bound; it inserts itself.
        assert_eq!(bindings.lookup(&typed(b"a")), unbound());
    }

    #[test]
    fn finds_bindings_of_keys_as_strokes_or_bytes() -> Result<(), std::io::Error> {
        let mut bindings = VtKeyBindings::default();
        bindings.bind(KeySequence::Strokes(vec![ctrl('x')]), command("search"))?;
        bindings.bind(
            KeySequence::Bytes(vec![b"\x1b[A".to_vec()]),
            KeyAction::DoInputFunction(InputFunction::AcceptLine),
        )?;

        assert_eq!(bindings.lookup(&typed(b"\x18")), bound_command("search"));
        assert_eq!(
            bindings.lookup(&typed(b"\x1b[A")).bound,
            Some(Binding::Function(InputFunction::AcceptLine))
        );
        // The same key, sent differently, is the default binding's.
        assert_eq!(
            bindings.lookup(&typed(b"\x1bOA")).bound,
            Some(Binding::Function(InputFunction::PreviousHistory))
        );

        Ok(())
    }

    #[test]
    fn binds_keys_that_send_the_same_bytes_as_others() -> Result<(), std::io::Error> {
        // `bind` records \C-i, \C-m, and \C-? as Ctrl strokes, but terminals send
        // them as Tab, Enter, and Backspace; and \C-h is 0x08.
        let mut bindings = VtKeyBindings::default();
        for (c, name) in [
            ('i', "tab"),
            ('m', "enter"),
            ('?', "rubout"),
            ('h', "ctrl-h"),
        ] {
            bindings.bind(KeySequence::Strokes(vec![ctrl(c)]), command(name))?;
        }
        for (bytes, name) in [
            (&b"\t"[..], "tab"),
            (b"\r", "enter"),
            (b"\x7f", "rubout"),
            (b"\x08", "ctrl-h"),
        ] {
            assert_eq!(
                bindings.lookup(&typed(bytes)),
                bound_command(name),
                "{name}"
            );
        }
        Ok(())
    }

    #[test]
    fn binds_keys_beyond_the_editing_keys() -> Result<(), std::io::Error> {
        let mut bindings = VtKeyBindings::default();
        let keys = [
            (ctrl(']'), &b"\x1d"[..]),
            (ctrl('_'), b"\x1f"),
            (ctrl('@'), b"\x00"),
            (Key::BackTab.into(), b"\x1b[Z"),
            (Key::PageUp.into(), b"\x1b[5~"),
            (Key::Insert.into(), b"\x1b[2~"),
            (Key::F(5).into(), b"\x1b[15~"),
        ];
        for (index, (stroke, _)) in keys.iter().enumerate() {
            bindings.bind(
                KeySequence::Strokes(vec![stroke.clone()]),
                command(&index.to_string()),
            )?;
        }
        for (index, (stroke, bytes)) in keys.iter().enumerate() {
            assert_eq!(
                bindings.lookup(&typed(bytes)),
                bound_command(&index.to_string()),
                "{stroke:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn uses_the_latest_of_a_binding_and_a_macro() -> Result<(), std::io::Error> {
        let mut bindings = VtKeyBindings::default();
        let ctrl_o = KeySequence::Strokes(vec![ctrl('o')]);

        bindings.bind(ctrl_o.clone(), command("cmd"))?;
        bindings.define_macro(ctrl_o.clone(), KeySequence::Bytes(vec![b"zz".to_vec()]))?;
        assert_eq!(
            bindings.lookup(&typed(b"\x0f")).bound,
            Some(Binding::Macro(b"zz".to_vec()))
        );
        assert!(!bindings.get_current().contains_key(&ctrl_o));

        bindings.bind(ctrl_o, command("again"))?;
        assert_eq!(bindings.lookup(&typed(b"\x0f")), bound_command("again"));
        assert!(bindings.get_macros().is_empty());

        // A later binding of a key that sends the same bytes wins too.
        bindings.bind(KeySequence::Strokes(vec![ctrl('i')]), command("tab"))?;
        assert_eq!(bindings.lookup(&typed(b"\t")), bound_command("tab"));
        bindings.bind(
            KeySequence::Strokes(vec![Key::Tab.into()]),
            KeyAction::DoInputFunction(InputFunction::Complete),
        )?;
        assert_eq!(
            bindings.lookup(&typed(b"\t")).bound,
            Some(Binding::Function(InputFunction::Complete))
        );

        Ok(())
    }

    #[test]
    fn unbinds_defaults_too() {
        let mut bindings = VtKeyBindings::default();
        assert!(bindings.try_unbind(KeySequence::Strokes(vec![ctrl('a')])));
        assert!(!bindings.try_unbind(KeySequence::Strokes(vec![ctrl('a')])));
        assert_eq!(bindings.lookup(&typed(b"\x01")), unbound());
    }

    #[test]
    fn rebinds_and_unbinds_a_key_however_its_written() -> Result<(), std::io::Error> {
        // Enter is bound by default; \C-m is another way to write it.
        let mut bindings = VtKeyBindings::default();
        let ctrl_m = KeySequence::Strokes(vec![ctrl('m')]);
        bindings.bind(ctrl_m.clone(), command("enter"))?;
        assert!(
            !bindings
                .get_current()
                .contains_key(&KeySequence::Strokes(vec![Key::Enter.into()]))
        );

        assert!(bindings.try_unbind(ctrl_m));
        assert_eq!(bindings.lookup(&typed(b"\r")), unbound());

        // Up can be sent other ways too, so binding one of them leaves the rest.
        bindings.bind(KeySequence::Bytes(vec![b"\x1b[A".to_vec()]), command("up"))?;
        assert_eq!(
            bindings.lookup(&typed(b"\x1bOA")).bound,
            Some(Binding::Function(InputFunction::PreviousHistory))
        );
        Ok(())
    }

    #[test]
    fn keeps_bindings_and_macros_for_bind() -> Result<(), std::io::Error> {
        let mut bindings = VtKeyBindings::default();
        let ctrl_x = KeySequence::Strokes(vec![ctrl('x')]);
        let up = KeySequence::Bytes(vec![b"\x1b[A".to_vec()]);

        bindings.bind(ctrl_x.clone(), command("cmd"))?;
        bindings.bind(up, command("cmd"))?;
        bindings.define_macro(KeySequence::Strokes(vec![ctrl('o')]), ctrl_x.clone())?;

        assert_eq!(bindings.get_current().get(&ctrl_x), Some(&command("cmd")));
        assert_eq!(bindings.get_untranslated(b"\x1b[A"), Some(&command("cmd")));
        assert_eq!(bindings.get_untranslated(b"\x1b[B"), None);
        assert_eq!(
            bindings.get_macros(),
            HashMap::from([(KeySequence::Strokes(vec![ctrl('o')]), ctrl_x)])
        );
        Ok(())
    }

    #[test]
    fn finds_sequences_and_macros() -> Result<(), std::io::Error> {
        let mut bindings = VtKeyBindings::default();
        // As atuin's bash integration binds them.
        let chained = KeySequence::Bytes(vec![
            vec![0x18],
            vec![0x1f],
            b"A".to_vec(),
            b"1".to_vec(),
            vec![0x07],
        ]);
        bindings.bind(chained.clone(), command("widget"))?;
        bindings.define_macro(KeySequence::Strokes(vec![ctrl('r')]), chained)?;

        assert_eq!(
            bindings.lookup(&typed(b"\x12")).bound,
            Some(Binding::Macro(b"\x18\x1fA1\x07".to_vec()))
        );
        assert_eq!(
            bindings.lookup(&typed(b"\x18\x1f")),
            Lookup {
                bound: None,
                prefix: true
            }
        );
        assert_eq!(
            bindings.lookup(&typed(b"\x18\x1fA1\x07")),
            bound_command("widget")
        );
        assert_eq!(bindings.lookup(&typed(b"\x18e")), unbound());
        Ok(())
    }

    #[test]
    fn reports_keys_both_bound_and_starting_a_longer_sequence() -> Result<(), std::io::Error> {
        let mut bindings = VtKeyBindings::default();
        bindings.bind(KeySequence::Strokes(vec![ctrl('x')]), command("short"))?;
        bindings.bind(
            KeySequence::Strokes(vec![ctrl('x'), ctrl('e')]),
            command("long"),
        )?;
        assert_eq!(
            bindings.lookup(&typed(b"\x18")),
            Lookup {
                bound: Some(Binding::Command("short".into())),
                prefix: true
            }
        );
        assert_eq!(bindings.lookup(&typed(b"\x18\x05")), bound_command("long"));
        // More keys than a sequence has don't match it.
        assert_eq!(bindings.lookup(&typed(b"\x18\x05a")), unbound());
        Ok(())
    }

    #[test]
    fn ignores_sequences_of_actions() -> Result<(), std::io::Error> {
        let mut bindings = VtKeyBindings::default();
        bindings.bind(
            KeySequence::Strokes(vec![ctrl('x')]),
            KeyAction::Sequence(vec![command("cmd")]),
        )?;
        assert_eq!(bindings.lookup(&typed(b"\x18")), unbound());
        Ok(())
    }

    #[test]
    fn expands_macros_to_the_bytes_a_terminal_sends() {
        assert_eq!(
            sequence_bytes(&KeySequence::Strokes(vec![ctrl('a'), Key::Home.into()])),
            b"\x01\x1b[H"
        );
        assert_eq!(
            sequence_bytes(&KeySequence::Bytes(vec![b"\x18".to_vec(), b"e".to_vec()])),
            b"\x18e"
        );
    }
}
