//! Conversions between the bytes a terminal sends for a key and the key as reedline
//! identifies it, plus the canonical spelling of a byte sequence.
//!
//! Everything a byte sequence is looked up against lives here: the platform's terminal
//! database (terminfo on unix, nothing elsewhere) and readline's default key table. These
//! are pure functions of their input; nothing here knows about bindings.

use brush_core::interfaces::KeySequence;
use reedline::{KeyCode, KeyModifiers};

/// The key sequences terminfo describes for the terminal we are attached to.
#[cfg(unix)]
mod terminal_database {
    use reedline::KeyCode;
    use std::collections::HashMap;
    use std::sync::LazyLock;
    use terminfo::capability as cap;

    macro_rules! key {
        ( $terminfo:expr , $our_key:expr, $terminfo_key:ty ) => {{
            (
                $our_key,
                $terminfo
                    .get::<$terminfo_key>()
                    .map(|k| k.expand().to_vec()),
            )
        }};
    }

    fn build_terminfo_key_map() -> HashMap<Vec<u8>, KeyCode> {
        let mut map: HashMap<Vec<u8>, KeyCode> = HashMap::new();

        if let Ok(ti) = terminfo::Database::from_env() {
            // Iterate over key capabilities and populate the map
            let key_capabilities = [
                key!(ti, KeyCode::F(1), cap::KeyF1<'_>),
                key!(ti, KeyCode::F(2), cap::KeyF2<'_>),
                key!(ti, KeyCode::F(3), cap::KeyF3<'_>),
                key!(ti, KeyCode::F(4), cap::KeyF4<'_>),
                key!(ti, KeyCode::F(5), cap::KeyF5<'_>),
                key!(ti, KeyCode::F(6), cap::KeyF6<'_>),
                key!(ti, KeyCode::F(7), cap::KeyF7<'_>),
                key!(ti, KeyCode::F(8), cap::KeyF8<'_>),
                key!(ti, KeyCode::F(9), cap::KeyF9<'_>),
                key!(ti, KeyCode::F(10), cap::KeyF10<'_>),
                key!(ti, KeyCode::F(11), cap::KeyF11<'_>),
                key!(ti, KeyCode::F(12), cap::KeyF12<'_>),
                key!(ti, KeyCode::Backspace, cap::KeyBackspace<'_>),
                key!(ti, KeyCode::Enter, cap::KeyEnter<'_>),
                key!(ti, KeyCode::Left, cap::KeyLeft<'_>),
                key!(ti, KeyCode::Right, cap::KeyRight<'_>),
                key!(ti, KeyCode::Up, cap::KeyUp<'_>),
                key!(ti, KeyCode::Down, cap::KeyDown<'_>),
                key!(ti, KeyCode::Home, cap::KeyHome<'_>),
                key!(ti, KeyCode::End, cap::KeyEnd<'_>),
                key!(ti, KeyCode::PageUp, cap::KeyPPage<'_>),
                key!(ti, KeyCode::PageDown, cap::KeyNPage<'_>),
                key!(ti, KeyCode::BackTab, cap::BackTab<'_>),
                // It's not clear if these belong here, because they're not
                // strictly "key" capabilities.
                key!(ti, KeyCode::Up, cap::CursorUp<'_>),
                key!(ti, KeyCode::Down, cap::CursorDown<'_>),
                key!(ti, KeyCode::Left, cap::CursorLeft<'_>),
                key!(ti, KeyCode::Right, cap::CursorRight<'_>),
            ];

            for (key, v) in key_capabilities {
                if let Some(Ok(v)) = v {
                    map.insert(v, key);
                }
            }
        }

        map
    }

    static TERMINFO_KEY_MAP: LazyLock<HashMap<Vec<u8>, KeyCode>> =
        LazyLock::new(build_terminfo_key_map);

    /// Looks `bytes` up among the key sequences terminfo describes for this terminal.
    pub(super) fn key_for_sequence(bytes: &[u8]) -> Option<KeyCode> {
        TERMINFO_KEY_MAP.get(bytes).copied()
    }
}

/// This platform has no terminal database.
#[cfg(not(unix))]
mod terminal_database {
    /// Looks `bytes` up in the terminal database; there is none here.
    pub(super) const fn key_for_sequence(_bytes: &[u8]) -> Option<reedline::KeyCode> {
        None
    }
}

/// Key sequences readline's default keymap accepts regardless of terminfo. Terminals send
/// the CSI (`\e[`) or SS3 (`\eO`) form depending on cursor mode, and terminfo describes only
/// one of them, so every platform's key lookup falls back to these.
const DEFAULT_SEQUENCES: &[(&[u8], KeyCode)] = &[
    (b"\x1b[A", KeyCode::Up),
    (b"\x1b[B", KeyCode::Down),
    (b"\x1b[C", KeyCode::Right),
    (b"\x1b[D", KeyCode::Left),
    (b"\x1b[H", KeyCode::Home),
    (b"\x1b[F", KeyCode::End),
    (b"\x1bOA", KeyCode::Up),
    (b"\x1bOB", KeyCode::Down),
    (b"\x1bOC", KeyCode::Right),
    (b"\x1bOD", KeyCode::Left),
    (b"\x1bOH", KeyCode::Home),
    (b"\x1bOF", KeyCode::End),
    (b"\x1b[1~", KeyCode::Home),
    (b"\x1b[2~", KeyCode::Insert),
    (b"\x1b[3~", KeyCode::Delete),
    (b"\x1b[4~", KeyCode::End),
    (b"\x1b[5~", KeyCode::PageUp),
    (b"\x1b[6~", KeyCode::PageDown),
    (b"\x1b[Z", KeyCode::BackTab),
    (b"\x1bOP", KeyCode::F(1)),
    (b"\x1bOQ", KeyCode::F(2)),
    (b"\x1bOR", KeyCode::F(3)),
    (b"\x1bOS", KeyCode::F(4)),
    (b"\x1b[15~", KeyCode::F(5)),
    (b"\x1b[17~", KeyCode::F(6)),
    (b"\x1b[18~", KeyCode::F(7)),
    (b"\x1b[19~", KeyCode::F(8)),
    (b"\x1b[20~", KeyCode::F(9)),
    (b"\x1b[21~", KeyCode::F(10)),
    (b"\x1b[23~", KeyCode::F(11)),
    (b"\x1b[24~", KeyCode::F(12)),
];

/// Looks `bytes` up among the key sequences every terminal is assumed to send.
fn key_from_default_sequence(bytes: &[u8]) -> Option<KeyCode> {
    DEFAULT_SEQUENCES
        .iter()
        .find(|(seq, _)| *seq == bytes)
        .map(|(_, key)| *key)
}

/// The byte sequence `key` is spelled with: the first default sequence that produces it.
fn default_sequence_for_key(key: KeyCode) -> Option<&'static [u8]> {
    DEFAULT_SEQUENCES
        .iter()
        .find(|(_, candidate)| *candidate == key)
        .map(|(seq, _)| *seq)
}

/// The key the terminal sends `bytes` for, when it is more than one byte: the platform's
/// terminal database first, then the default table. Single bytes are decoded by [`lift_key`]
/// itself, which knows the control and meta forms no table lists.
fn key_for_sequence(bytes: &[u8]) -> Option<KeyCode> {
    if bytes.len() < 2 {
        return None;
    }

    terminal_database::key_for_sequence(bytes).or_else(|| key_from_default_sequence(bytes))
}

/// A key as reedline identifies it.
pub(super) type ReedlineKey = (KeyModifiers, KeyCode);

/// Longest byte sequence treated as one key. Bounds the scan in [`decode_key`]; a longer
/// terminfo entry is not a key here, so it binds and replays as a raw sequence.
pub(super) const MAX_KEY_LEN: usize = 8;

/// Finds the longest prefix of `bytes` that the terminal would report as one key.
pub(super) fn decode_key(bytes: &[u8]) -> Option<(ReedlineKey, usize)> {
    (1..=bytes.len().min(MAX_KEY_LEN))
        .rev()
        .find_map(|len| Some((lift_key(&bytes[..len])?, len)))
}

/// The canonical form of a key sequence: each part the terminal would report as one key is
/// replaced by the bytes that key is spelled with, so `\eOA` and `\e[A` name the same key
/// and `\C-x\eOA` the same two-key sequence as `\C-x\e[A`. Bytes that lift to no key are
/// kept as they are.
pub(super) fn canonical(seq: &KeySequence) -> KeySequence {
    let mut bytes = Vec::with_capacity(seq.as_bytes().len());
    let mut rest = seq.as_bytes();

    while !rest.is_empty() {
        let spelled = decode_key(rest).and_then(|((modifiers, key_code), len)| {
            Some((key_sequence_for(modifiers, key_code)?, len))
        });
        if let Some((spelled, len)) = spelled {
            bytes.extend(spelled.into_bytes());
            rest = &rest[len..];
        } else {
            bytes.push(rest[0]);
            rest = &rest[1..];
        }
    }

    KeySequence::from(bytes)
}

/// Lifts a byte sequence to the key crossterm reports for it in raw mode, if it is exactly
/// one key. Terminal key sequences come from the same terminfo-backed table `bind` uses.
pub(super) fn lift_key(bytes: &[u8]) -> Option<ReedlineKey> {
    if bytes.len() > MAX_KEY_LEN {
        return None;
    }
    if let Some(key_code) = key_for_sequence(bytes) {
        // Shift is part of what makes back-tab back-tab, and the terminal reports it so.
        let modifiers = if key_code == KeyCode::BackTab {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        return Some((modifiers, key_code));
    }

    if let Some(key) = lift_modified_key(bytes) {
        return Some(key);
    }

    let key = match *bytes {
        [] => return None,
        [0x1b] => (KeyModifiers::NONE, KeyCode::Esc),
        // The terminal reads ESC ESC as the escape key and what follows separately.
        [0x1b, 0x1b, ..] => return None,
        [0x1b, ref rest @ ..] => {
            let (modifiers, key_code) = lift_key(rest)?;
            (modifiers | KeyModifiers::ALT, key_code)
        }
        [b'\r'] => (KeyModifiers::NONE, KeyCode::Enter),
        [b'\t'] => (KeyModifiers::NONE, KeyCode::Tab),
        [0x7f] => (KeyModifiers::NONE, KeyCode::Backspace),
        [0x00] => (KeyModifiers::CONTROL, KeyCode::Char(' ')),
        [c @ 0x01..=0x1a] => (
            KeyModifiers::CONTROL,
            KeyCode::Char((c - 0x01 + b'a') as char),
        ),
        [c @ 0x1c..=0x1f] => (
            KeyModifiers::CONTROL,
            KeyCode::Char((c - 0x1c + b'4') as char),
        ),
        _ => {
            let mut chars = std::str::from_utf8(bytes).ok()?.chars();
            let character = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            (KeyModifiers::NONE, KeyCode::Char(character))
        }
    };

    Some(key)
}

/// Lifts the forms terminals use for modified keys: `ESC [ 1 ; m X` for cursor and function
/// keys and `ESC [ n ; m ~` for the tilde keys, where `m` is one more than a mask of
/// shift (1), alt (2) and control (4).
fn lift_modified_key(bytes: &[u8]) -> Option<ReedlineKey> {
    let (last, params) = bytes.strip_prefix(b"\x1b[")?.split_last()?;
    let (first, modifier) = std::str::from_utf8(params).ok()?.split_once(';')?;
    let modifier: u8 = modifier.parse().ok()?;

    let plain: Vec<u8> = match (*last, first) {
        (b'~', number) => std::format!("\x1b[{number}~").into_bytes(),
        (letter, "1") => {
            let csi = [0x1b, b'[', letter];
            let ss3 = [0x1b, b'O', letter];
            if key_from_default_sequence(&csi).is_some() {
                csi.to_vec()
            } else {
                ss3.to_vec()
            }
        }
        _ => return None,
    };
    let key_code = key_from_default_sequence(&plain)?;

    Some((modifiers_from_parameter(modifier), key_code))
}

fn modifiers_from_parameter(parameter: u8) -> KeyModifiers {
    let mask = parameter.saturating_sub(1);
    let mut modifiers = KeyModifiers::empty();
    modifiers.set(KeyModifiers::SHIFT, mask & 1 != 0);
    modifiers.set(KeyModifiers::ALT, mask & 2 != 0);
    modifiers.set(KeyModifiers::CONTROL, mask & 4 != 0);
    modifiers
}

const fn modifier_parameter(modifiers: KeyModifiers) -> u8 {
    let mut mask = 0;
    if modifiers.contains(KeyModifiers::SHIFT) {
        mask |= 1;
    }
    if modifiers.contains(KeyModifiers::ALT) {
        mask |= 2;
    }
    if modifiers.contains(KeyModifiers::CONTROL) {
        mask |= 4;
    }
    mask + 1
}

/// The inverse of [`lift_key`]: the bytes a key is spelled with. SHIFT is ignored on
/// characters, since the character already carries its case and `bind` never records it.
pub(super) fn key_sequence_for(modifiers: KeyModifiers, key_code: KeyCode) -> Option<KeySequence> {
    let is_terminal_key = !matches!(
        key_code,
        KeyCode::Char(_) | KeyCode::Enter | KeyCode::Tab | KeyCode::Backspace | KeyCode::Esc
    );
    if is_terminal_key {
        return key_sequence_for_terminal_key(modifiers, key_code);
    }

    let mut bytes = Vec::new();
    if modifiers.contains(KeyModifiers::ALT) {
        bytes.push(0x1b);
    }

    match key_code {
        KeyCode::Char(c) if modifiers.contains(KeyModifiers::CONTROL) => {
            bytes.push(match c.to_ascii_lowercase() {
                ' ' | '@' => 0x00,
                c @ 'a'..='z' => c as u8 - b'a' + 0x01,
                c @ '4'..='7' => c as u8 - b'4' + 0x1c,
                '\\' | ']' | '^' | '_' | '?' => {
                    brush_parser::readline_binding::control_byte(c as u8)
                }
                _ => return None,
            });
        }
        KeyCode::Char(c) => {
            let mut utf8 = [0; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut utf8).as_bytes());
        }
        KeyCode::Enter => bytes.push(b'\r'),
        KeyCode::Tab => bytes.push(b'\t'),
        KeyCode::Backspace => bytes.push(0x7f),
        KeyCode::Esc => bytes.push(0x1b),
        _ => return None,
    }

    Some(KeySequence::from(bytes))
}

/// A terminal key's byte spelling, only when it preserves the key's editing meaning.
///
/// Shift on printable characters only records their case; control/shift combinations and
/// modified Enter, Tab or Backspace can instead have distinct native editor bindings.
pub(super) fn input_sequence_for(
    modifiers: KeyModifiers,
    key_code: KeyCode,
) -> Option<KeySequence> {
    let sequence = key_sequence_for(modifiers, key_code)?;
    let (decoded, _) = lift_key(sequence.as_bytes())?;
    let mut expected = modifiers;
    if matches!(key_code, KeyCode::Char(_)) && !modifiers.contains(KeyModifiers::CONTROL) {
        expected.remove(KeyModifiers::SHIFT);
    }

    (decoded == expected).then_some(sequence)
}

/// Spells a cursor, function or tilde key, with modifiers folded in the way terminals send
/// them (the inverse of [`lift_modified_key`]).
fn key_sequence_for_terminal_key(
    modifiers: KeyModifiers,
    key_code: KeyCode,
) -> Option<KeySequence> {
    let plain = default_sequence_for_key(key_code)?;

    // Shift is part of what makes back-tab back-tab, not a modifier on top of it.
    let mut modifiers =
        modifiers & (KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL);
    if key_code == KeyCode::BackTab {
        modifiers.remove(KeyModifiers::SHIFT);
    }
    if modifiers.is_empty() {
        return Some(KeySequence::from(plain.to_vec()));
    }

    let parameter = modifier_parameter(modifiers);
    let spelled = match plain {
        [0x1b, b'[' | b'O', letter] => std::format!("\x1b[1;{parameter}{}", *letter as char),
        [0x1b, b'[', digits @ .., b'~'] => {
            std::format!("\x1b[{};{parameter}~", std::str::from_utf8(digits).ok()?)
        }
        _ => return None,
    };

    Some(KeySequence::from(spelled.into_bytes()))
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use super::*;

    #[test]
    fn lift_and_spell_are_inverses() -> Result<(), std::io::Error> {
        use KeyModifiers as M;

        // Everything the terminal can send as one key, as bytes: every single byte, every
        // default terminal sequence with every modifier combination, and meta forms.
        let mut corpus: Vec<Vec<u8>> = (0u8..=0x7f).map(|b| vec![b]).collect();
        corpus.extend((0u8..=0x7f).map(|b| vec![0x1b, b]));
        corpus.extend(
            ["ü", "€", "A", "["]
                .into_iter()
                .flat_map(|c| [c.as_bytes().to_vec(), [b"\x1b", c.as_bytes()].concat()]),
        );
        for (seq, _) in DEFAULT_SEQUENCES {
            corpus.push(seq.to_vec());
            if let Some((_, key_code)) = lift_key(seq) {
                for modifiers in [M::SHIFT, M::ALT, M::CONTROL, M::ALT | M::CONTROL] {
                    if let Some(spelled) = key_sequence_for(modifiers, key_code) {
                        corpus.push(spelled.into_bytes());
                    }
                }
            }
        }

        for bytes in corpus {
            let Some(key) = lift_key(&bytes) else {
                continue;
            };
            let spelled = key_sequence_for(key.0, key.1).ok_or_else(|| {
                std::io::Error::other(std::format!("{key:?} (from {bytes:?}) has no spelling"))
            })?;
            assert_eq!(
                lift_key(spelled.as_bytes()),
                Some(key),
                "spelling {spelled} of {key:?} (from {bytes:?}) lifts differently"
            );
            assert_eq!(
                canonical(&KeySequence::from(bytes.clone())),
                spelled,
                "canonical form of {bytes:?} is not its key's spelling"
            );
        }

        Ok(())
    }

    #[test]
    fn default_sequences_cover_both_cursor_modes() {
        // Terminals send the CSI or SS3 form depending on cursor mode, and terminfo lists
        // only one of them; readline's default keymap accepts both, and so must we.
        for (bytes, key_code) in [
            (&b"\x1b[A"[..], KeyCode::Up),
            (b"\x1bOA", KeyCode::Up),
            (b"\x1b[D", KeyCode::Left),
            (b"\x1bOD", KeyCode::Left),
            (b"\x1b[H", KeyCode::Home),
            (b"\x1b[F", KeyCode::End),
            (b"\x1b[3~", KeyCode::Delete),
            (b"\x1b[5~", KeyCode::PageUp),
            (b"\x1b[Z", KeyCode::BackTab),
        ] {
            assert_eq!(
                key_from_default_sequence(bytes),
                Some(key_code),
                "{bytes:?}"
            );
            assert_eq!(key_for_sequence(bytes), Some(key_code), "{bytes:?}");
        }
        assert_eq!(key_from_default_sequence(b"\x1b[1;5C"), None);
        assert_eq!(key_from_default_sequence(b"a"), None);
    }

    #[test]
    fn every_default_key_spells_back_to_a_default_sequence() {
        for (seq, key_code) in DEFAULT_SEQUENCES {
            assert!(
                default_sequence_for_key(*key_code).is_some(),
                "{key_code:?} has no spelling"
            );
            assert_eq!(key_from_default_sequence(seq), Some(*key_code));
        }
    }

    #[test]
    fn a_single_byte_is_never_looked_up_in_a_table() {
        // Single bytes are `lift_key`'s own business: it knows the control and meta forms
        // that no key table lists, and a one-byte terminfo entry must not shadow them.
        for byte in 0..=u8::MAX {
            assert_eq!(key_for_sequence(&[byte]), None, "0x{byte:02x}");
        }
        assert_eq!(key_for_sequence(b""), None);
        assert_eq!(
            lift_key(b"\x7f"),
            Some((KeyModifiers::NONE, KeyCode::Backspace))
        );
        assert_eq!(
            lift_key(b"a"),
            Some((KeyModifiers::NONE, KeyCode::Char('a')))
        );
    }

    #[test]
    fn every_default_sequence_fits_the_key_length_bound() {
        for (seq, key) in DEFAULT_SEQUENCES {
            assert!(
                seq.len() <= MAX_KEY_LEN,
                "{key:?} ({seq:?}) exceeds MAX_KEY_LEN"
            );
        }
        for modifiers in [
            KeyModifiers::CONTROL,
            KeyModifiers::ALT | KeyModifiers::CONTROL,
        ] {
            let spelled = key_sequence_for(modifiers, KeyCode::F(12)).map(KeySequence::into_bytes);
            assert!(
                spelled.as_ref().is_some_and(|s| s.len() <= MAX_KEY_LEN),
                "{spelled:?}"
            );
        }
        // The bound applies to binding a key exactly as it does to decoding one: a longer
        // sequence is not one key, so it binds raw and decodes as its leading key.
        let too_long = b"\x1b[12345;5~";
        assert_eq!(lift_key(too_long), None);
        assert_ne!(
            decode_key(too_long).map(|(_, len)| len),
            Some(too_long.len())
        );
    }

    #[test]
    fn a_run_of_escapes_is_not_one_key() {
        // The terminal reports ESC ESC as the escape key and what follows separately, so
        // \e\ef is two keys and must not alias \M-f.
        assert_eq!(lift_key(b"\x1b\x1bf"), None);
        assert_eq!(lift_key(b"\x1b\x1b"), None);
        assert_ne!(
            canonical(&KeySequence::from(b"\x1b\x1bf".to_vec())),
            canonical(&KeySequence::from(b"\x1bf".to_vec()))
        );
        assert_eq!(
            canonical(&KeySequence::from(b"\x1b\x1bf".to_vec())),
            KeySequence::from(b"\x1b\x1bf".to_vec())
        );
    }

    #[test]
    fn canonical_respells_each_key_of_a_raw_sequence() {
        // A multi-key sequence is canonicalized key by key: the SS3 up arrow after \C-x
        // becomes the CSI form, and text and control bytes stay as they are.
        assert_eq!(
            canonical(&KeySequence::from(b"\x18\x1bOA".to_vec())),
            KeySequence::from(b"\x18\x1b[A".to_vec())
        );
        assert_eq!(
            canonical(&KeySequence::from(b"\x1b[1;5~x".to_vec())),
            KeySequence::from(b"\x1b[1;5Hx".to_vec())
        );
        assert_eq!(
            canonical(&KeySequence::from(b"\x18\x1fA1\x07".to_vec())),
            KeySequence::from(b"\x18\x1fA1\x07".to_vec())
        );
        // Multi-byte text and an undecodable byte pass through untouched.
        assert_eq!(
            canonical(&KeySequence::from("é\u{ff}".as_bytes().to_vec())),
            KeySequence::from("é\u{ff}".as_bytes().to_vec())
        );
        assert_eq!(
            canonical(&KeySequence::from(b"a\xffb".to_vec())),
            KeySequence::from(b"a\xffb".to_vec())
        );
    }

    #[test]
    fn control_punctuation_spells_as_readline_does() {
        for (character, byte) in [
            (' ', 0x00),
            ('@', 0x00),
            ('4', 0x1c),
            ('_', 0x1f),
            ('?', 0x7f),
        ] {
            assert_eq!(
                key_sequence_for(KeyModifiers::CONTROL, KeyCode::Char(character)),
                Some(KeySequence::from(vec![byte])),
                "\\C-{character}"
            );
        }
    }

    #[test]
    fn input_spelling_preserves_distinct_native_modifiers() {
        for (modifiers, code) in [
            (KeyModifiers::SHIFT, KeyCode::Enter),
            (KeyModifiers::CONTROL, KeyCode::Backspace),
            (
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                KeyCode::Char('a'),
            ),
            (KeyModifiers::SUPER, KeyCode::Char('a')),
        ] {
            assert_eq!(input_sequence_for(modifiers, code), None);
        }
        for (modifiers, code, bytes) in [
            (KeyModifiers::SHIFT, KeyCode::Char('A'), b"A".as_slice()),
            (
                KeyModifiers::ALT | KeyModifiers::SHIFT,
                KeyCode::Char('F'),
                b"\x1bF".as_slice(),
            ),
            (
                KeyModifiers::CONTROL,
                KeyCode::Char('_'),
                b"\x1f".as_slice(),
            ),
        ] {
            assert_eq!(
                input_sequence_for(modifiers, code),
                Some(KeySequence::from(bytes.to_vec()))
            );
        }
    }

    #[test]
    fn input_spelling_never_discards_meaningful_modifiers() {
        for bits in 0..=KeyModifiers::all().bits() {
            let modifiers = KeyModifiers::from_bits_retain(bits);
            for code in [
                KeyCode::Char('a'),
                KeyCode::Char('A'),
                KeyCode::Char('_'),
                KeyCode::Char('?'),
                KeyCode::Char('\u{e9}'),
                KeyCode::Enter,
                KeyCode::Tab,
                KeyCode::BackTab,
                KeyCode::Backspace,
                KeyCode::Esc,
                KeyCode::Up,
                KeyCode::F(1),
                KeyCode::Delete,
            ] {
                let Some(sequence) = input_sequence_for(modifiers, code) else {
                    continue;
                };
                let (decoded, _) = lift_key(sequence.as_bytes()).unwrap();
                let mut expected = modifiers;
                if matches!(code, KeyCode::Char(_)) && !modifiers.contains(KeyModifiers::CONTROL) {
                    expected.remove(KeyModifiers::SHIFT);
                }
                assert_eq!(decoded, expected, "{modifiers:?} {code:?}");
            }
        }
    }

    #[test]
    fn modified_terminal_keys_lift_and_spell() {
        assert_eq!(
            lift_key(b"\x1b[1;5C"),
            Some((KeyModifiers::CONTROL, KeyCode::Right))
        );
        assert_eq!(
            lift_key(b"\x1b[3;3~"),
            Some((KeyModifiers::ALT, KeyCode::Delete))
        );
        assert_eq!(
            lift_key(b"\x1b[1;6P"),
            Some((KeyModifiers::SHIFT | KeyModifiers::CONTROL, KeyCode::F(1)))
        );
        assert_eq!(
            key_sequence_for(KeyModifiers::CONTROL, KeyCode::Delete),
            Some(KeySequence::from(b"\x1b[3;5~".to_vec()))
        );
        assert_eq!(
            key_sequence_for(KeyModifiers::SHIFT, KeyCode::BackTab),
            Some(KeySequence::from(b"\x1b[Z".to_vec()))
        );
        assert_eq!(lift_key(b"\x1b[1;5"), None);
        assert_eq!(lift_key(b"\x1b[9;5~"), None);
    }

    #[test]
    fn decode_key_takes_the_longest_key() {
        assert_eq!(
            decode_key(b"\x1b[1;5Cx"),
            Some(((KeyModifiers::CONTROL, KeyCode::Right), 6))
        );
        assert_eq!(
            decode_key(b"\x1bfx"),
            Some(((KeyModifiers::ALT, KeyCode::Char('f')), 2))
        );
        assert_eq!(
            decode_key("éa".as_bytes()),
            Some(((KeyModifiers::NONE, KeyCode::Char('é')), 2))
        );
        assert_eq!(decode_key(b"\xff"), None);
    }
}
