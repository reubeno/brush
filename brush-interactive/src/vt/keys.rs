//! Keys: decoding them from the bytes a terminal sends, and encoding them back.

use std::collections::VecDeque;
use std::time::Duration;

use brush_core::interfaces::{Key, KeyStroke};

use super::terminal::Terminal;

/// How long to wait for the rest of a key sequence: readline's default
/// `keyseq-timeout`.
pub(crate) const KEYSEQ_TIMEOUT: Duration = Duration::from_millis(500);

const ESC: u8 = 0x1b;

/// A key pressed at the terminal (or typed by a macro).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Keypress {
    /// The key, or `None` for a sequence this backend doesn't recognize.
    pub stroke: Option<KeyStroke>,
    /// The bytes it arrived as.
    pub bytes: Vec<u8>,
    /// Whether a macro typed it.
    pub from_macro: bool,
}

impl Keypress {
    /// Returns the char this key types, if it's a plain character key.
    pub const fn typed_char(&self) -> Option<char> {
        match &self.stroke {
            Some(KeyStroke {
                alt: false,
                control: false,
                key: Key::Character(c),
                ..
            }) => Some(*c),
            _ => None,
        }
    }

    /// Whether this is the Escape key on its own.
    pub fn is_escape(&self) -> bool {
        self.stroke.as_ref() == Some(&KeyStroke::from(Key::Escape))
    }
}

/// Reads keys from a terminal. Bytes arrive in arbitrary chunks, so an escape
/// sequence or UTF-8 character may be split between them; the reader holds on to
/// an incomplete one until the rest arrives, waiting only briefly for the rest of
/// an escape sequence before taking what it has as keys of their own.
#[derive(Default)]
pub(crate) struct KeyReader {
    /// Bytes read but not yet decoded.
    pending: Vec<u8>,
    /// Keys to return before decoding any more (e.g., those a macro types).
    queued: VecDeque<Keypress>,
}

impl KeyReader {
    /// Returns the next key, waiting for one if need be; `None` at the end of
    /// input.
    pub fn next(&mut self, terminal: &mut impl Terminal) -> std::io::Result<Option<Keypress>> {
        loop {
            if let Some(key) = self.queued.pop_front() {
                return Ok(Some(key));
            }
            if let Some(key) = self.decode(true) {
                return Ok(Some(key));
            }

            // Part of a key sequence may have arrived without the rest. Wait a little
            // for it, as readline does, before taking what's there on its own (e.g.,
            // a lone Escape as the Escape key).
            if !self.pending.is_empty()
                && !terminal.wait_for_input(KEYSEQ_TIMEOUT)?
                && let Some(key) = self.decode(false)
            {
                return Ok(Some(key));
            }

            let mut bytes = [0; 256];
            let count = loop {
                match terminal.read(&mut bytes) {
                    Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
                    result => break result?,
                }
            };
            if count == 0 {
                return Ok(self.decode(false));
            }
            self.pending
                .extend_from_slice(bytes.get(..count).unwrap_or_default());
        }
    }

    /// Returns the next key if it arrives within `timeout`.
    pub fn next_within(
        &mut self,
        terminal: &mut impl Terminal,
        timeout: Duration,
    ) -> std::io::Result<Option<Keypress>> {
        if self.queued.is_empty() && self.pending.is_empty() && !terminal.wait_for_input(timeout)? {
            return Ok(None);
        }
        self.next(terminal)
    }

    /// Puts keys back, to be returned, in order, before any others.
    pub fn push_front(&mut self, keys: Vec<Keypress>) {
        for key in keys.into_iter().rev() {
            self.queued.push_front(key);
        }
    }

    /// Decodes the next key from the bytes read, if they hold a whole one.
    /// `more_coming` is as for [`decode`].
    fn decode(&mut self, more_coming: bool) -> Option<Keypress> {
        let (stroke, len) = decode(&self.pending, more_coming)?;
        Some(Keypress {
            stroke,
            bytes: self.pending.drain(..len).collect(),
            from_macro: false,
        })
    }
}

/// Decodes all the keys in `bytes` (e.g., a macro's), as if typed.
pub(crate) fn decode_all(mut bytes: &[u8]) -> Vec<Keypress> {
    let mut keys = vec![];
    while let Some((stroke, len)) = decode(bytes, false) {
        let (key, rest) = bytes.split_at(len);
        keys.push(Keypress {
            stroke,
            bytes: key.to_vec(),
            from_macro: true,
        });
        bytes = rest;
    }
    keys
}

/// Decodes the key at the start of `bytes`, returning it (or `None` if it isn't
/// one this backend recognizes) and how many bytes it took. Returns `None` if
/// `bytes` is empty or holds only the start of a key.
///
/// # Arguments
///
/// * `bytes` - The bytes to decode.
/// * `more_coming` - Whether more bytes may be about to arrive. When `false`, the
///   start of an escape sequence is taken to be keys of its own (e.g., a lone
///   Escape is the Escape key).
fn decode(bytes: &[u8], more_coming: bool) -> Option<(Option<KeyStroke>, usize)> {
    let &first = bytes.first()?;
    let stroke = match first {
        ESC => return decode_escape(bytes, more_coming),
        b'\r' => Key::Enter.into(),
        b'\t' => Key::Tab.into(),
        0x7f => Key::Backspace.into(),
        // Ctrl-A through Ctrl-Z (including Ctrl-H, Ctrl-J, and so on, which readline
        // binds as such).
        0x01..=0x1a => ctrl(char::from(first - 1 + b'a')),
        // Ctrl-@, Ctrl-\, Ctrl-], Ctrl-^, and Ctrl-_.
        0x00..=0x1f => ctrl(char::from(first + 0x40)),
        _ => {
            return decode_char(bytes).map(|(c, len)| (Some(Key::Character(c).into()), len));
        }
    };
    Some((Some(stroke), 1))
}

/// Decodes a UTF-8 character at the start of `bytes`.
fn decode_char(bytes: &[u8]) -> Option<(char, usize)> {
    let len = match bytes.first()? {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        // Not the start of a character; skip it.
        _ => return Some((char::REPLACEMENT_CHARACTER, 1)),
    };
    // Give up on a character as soon as a byte arrives that can't continue it,
    // rather than holding that byte back until the rest of the character would
    // have arrived.
    if bytes.iter().take(len).skip(1).any(|&b| b & 0xc0 != 0x80) {
        return Some((char::REPLACEMENT_CHARACTER, 1));
    }
    let encoded = bytes.get(..len)?;
    match std::str::from_utf8(encoded) {
        Ok(s) => s.chars().next().map(|c| (c, len)),
        Err(_) => Some((char::REPLACEMENT_CHARACTER, 1)),
    }
}

/// Decodes a key that starts with Escape.
fn decode_escape(bytes: &[u8], more_coming: bool) -> Option<(Option<KeyStroke>, usize)> {
    let Some(&second) = bytes.get(1) else {
        return (!more_coming).then(|| (Some(Key::Escape.into()), 1));
    };

    match second {
        b'[' => decode_csi(bytes, more_coming),
        b'O' => decode_ss3(bytes, more_coming),
        ESC => Some((Some(Key::Escape.into()), 1)),
        // Escape and then a key is that key with Alt (or Meta) held.
        _ => {
            let (stroke, len) = decode(bytes.get(1..)?, more_coming)?;
            Some((
                stroke.map(|stroke| KeyStroke {
                    alt: true,
                    ..stroke
                }),
                len + 1,
            ))
        }
    }
}

/// Decodes a control sequence: `ESC [`, parameter bytes (0x30–0x3f),
/// intermediate bytes (0x20–0x2f), and a final byte (0x40–0x7e). If what follows
/// `ESC [` can't be one, it's Alt-[.
fn decode_csi(bytes: &[u8], more_coming: bool) -> Option<(Option<KeyStroke>, usize)> {
    let alt_bracket = || Some((Some(alt('[')), 2));
    let body = bytes.get(2..)?;
    let Some(final_index) = body.iter().position(|b| !(0x20..=0x3f).contains(b)) else {
        return if more_coming { None } else { alt_bracket() };
    };
    let final_byte = body.get(final_index).copied()?;
    if !(0x40..=0x7e).contains(&final_byte) {
        return alt_bracket();
    }

    let params = std::str::from_utf8(body.get(..final_index)?).unwrap_or_default();
    let mut params = params.split(';').map(|p| p.parse::<u32>().ok());
    let number = params.next().flatten();
    // A modifier, if present, is the second parameter (e.g., `ESC [ 1 ; 5 C`): 1
    // plus 2 for Alt and 4 for Ctrl (and 1 for Shift, which `bind` can't express,
    // so it's left out).
    let modifier = params.next().flatten().unwrap_or(1).saturating_sub(1);

    let key = match (final_byte, number) {
        (b'A', _) => Some(Key::Up),
        (b'B', _) => Some(Key::Down),
        (b'C', _) => Some(Key::Right),
        (b'D', _) => Some(Key::Left),
        (b'H', _) => Some(Key::Home),
        (b'F', _) => Some(Key::End),
        (b'Z', _) => Some(Key::BackTab),
        (b'P', _) => Some(Key::F(1)),
        (b'Q', _) => Some(Key::F(2)),
        (b'R', _) => Some(Key::F(3)),
        (b'S', _) => Some(Key::F(4)),
        (b'~', Some(number)) => tilde_key(number),
        _ => None,
    };
    let stroke = key.map(|key| KeyStroke {
        alt: modifier & 2 != 0,
        control: modifier & 4 != 0,
        shift: false,
        key,
    });
    Some((stroke, 2 + final_index + 1))
}

/// Decodes a single-shift sequence: `ESC O` and a final byte, as cursor and
/// function keys send in application mode. If what follows `ESC O` can't be one,
/// it's Alt-O.
fn decode_ss3(bytes: &[u8], more_coming: bool) -> Option<(Option<KeyStroke>, usize)> {
    let Some(&third) = bytes.get(2) else {
        return (!more_coming).then(|| (Some(alt('O')), 2));
    };
    if !(0x40..=0x7e).contains(&third) {
        return Some((Some(alt('O')), 2));
    }

    let key = match third {
        b'A' => Some(Key::Up),
        b'B' => Some(Key::Down),
        b'C' => Some(Key::Right),
        b'D' => Some(Key::Left),
        b'H' => Some(Key::Home),
        b'F' => Some(Key::End),
        b'P' => Some(Key::F(1)),
        b'Q' => Some(Key::F(2)),
        b'R' => Some(Key::F(3)),
        b'S' => Some(Key::F(4)),
        _ => None,
    };
    Some((key.map(KeyStroke::from), 3))
}

/// The keys that send `ESC [ <number> ~`, by number, as xterm sends them.
const TILDE_KEYS: [(u32, Key); 16] = [
    (1, Key::Home),
    (2, Key::Insert),
    (3, Key::Delete),
    (4, Key::End),
    (5, Key::PageUp),
    (6, Key::PageDown),
    (15, Key::F(5)),
    (17, Key::F(6)),
    (18, Key::F(7)),
    (19, Key::F(8)),
    (20, Key::F(9)),
    (21, Key::F(10)),
    (23, Key::F(11)),
    (24, Key::F(12)),
    // Other terminals' Home and End.
    (7, Key::Home),
    (8, Key::End),
];

fn tilde_key(number: u32) -> Option<Key> {
    TILDE_KEYS
        .iter()
        .find(|(n, _)| *n == number)
        .map(|(_, key)| key.clone())
}

fn tilde_number(key: &Key) -> Option<u32> {
    TILDE_KEYS.iter().find(|(_, k)| k == key).map(|(n, _)| *n)
}

/// Returns the bytes a terminal (xterm, or one like it) sends for `stroke`, or
/// nothing if it doesn't send any this backend knows.
pub(crate) fn encode(stroke: &KeyStroke) -> Vec<u8> {
    let alt_prefix = if stroke.alt { &[ESC][..] } else { &[] };
    let modifier = 1 + 2 * u32::from(stroke.alt) + 4 * u32::from(stroke.control);
    // Cursor and function keys carry their modifiers as a parameter.
    let csi = |final_byte: char| {
        if modifier == 1 {
            format!("\x1b[{final_byte}").into_bytes()
        } else {
            format!("\x1b[1;{modifier}{final_byte}").into_bytes()
        }
    };

    match &stroke.key {
        Key::Character(c) if stroke.control && c.is_ascii() => {
            // Ctrl-? is Delete; other Ctrl characters keep the character's low bits.
            let byte = if *c == '?' { 0x7f } else { *c as u8 & 0x1f };
            [alt_prefix, &[byte]].concat()
        }
        Key::Character(c) => [alt_prefix, c.encode_utf8(&mut [0; 4]).as_bytes()].concat(),
        Key::Enter => [alt_prefix, b"\r"].concat(),
        Key::Tab => [alt_prefix, b"\t"].concat(),
        Key::Backspace => [alt_prefix, b"\x7f"].concat(),
        Key::Escape => [alt_prefix, &[ESC]].concat(),
        Key::BackTab => b"\x1b[Z".to_vec(),
        Key::Up => csi('A'),
        Key::Down => csi('B'),
        Key::Right => csi('C'),
        Key::Left => csi('D'),
        Key::Home => csi('H'),
        Key::End => csi('F'),
        Key::F(n @ 1..=4) => {
            let final_byte = char::from(b'P' + n - 1);
            if modifier == 1 {
                format!("\x1bO{final_byte}").into_bytes()
            } else {
                csi(final_byte)
            }
        }
        key => match tilde_number(key) {
            Some(number) if modifier == 1 => format!("\x1b[{number}~").into_bytes(),
            Some(number) => format!("\x1b[{number};{modifier}~").into_bytes(),
            None => vec![],
        },
    }
}

const fn ctrl(c: char) -> KeyStroke {
    KeyStroke {
        alt: false,
        control: true,
        shift: false,
        key: Key::Character(c),
    }
}

const fn alt(c: char) -> KeyStroke {
    KeyStroke {
        alt: true,
        control: false,
        shift: false,
        key: Key::Character(c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(
        clippy::unnecessary_wraps,
        reason = "what's decoded is compared as an Option"
    )]
    fn stroke(alt: bool, control: bool, key: Key) -> Option<KeyStroke> {
        Some(KeyStroke {
            alt,
            control,
            shift: false,
            key,
        })
    }

    fn plain(key: Key) -> Option<KeyStroke> {
        stroke(false, false, key)
    }

    fn ch(c: char) -> Option<KeyStroke> {
        plain(Key::Character(c))
    }

    fn keys(bytes: &[u8]) -> Vec<Option<KeyStroke>> {
        decode_all(bytes)
            .into_iter()
            .map(|key| key.stroke)
            .collect()
    }

    #[test]
    fn decodes_characters_and_controls() {
        assert_eq!(
            keys("aé界\r\t\x7f\x01\x12".as_bytes()),
            [
                ch('a'),
                ch('é'),
                ch('界'),
                plain(Key::Enter),
                plain(Key::Tab),
                plain(Key::Backspace),
                Some(ctrl('a')),
                Some(ctrl('r')),
            ]
        );
    }

    #[test]
    fn decodes_every_control_character_as_a_ctrl_key() {
        // Bytes that readline treats as Ctrl keys, rather than as other keys: \C-h
        // (0x08), \C-j (0x0a), and the controls past \C-z.
        assert_eq!(
            keys(b"\x08\n\x00\x1c\x1d\x1e\x1f"),
            [
                Some(ctrl('h')),
                Some(ctrl('j')),
                Some(ctrl('@')),
                Some(ctrl('\\')),
                Some(ctrl(']')),
                Some(ctrl('^')),
                Some(ctrl('_')),
            ]
        );
    }

    #[test]
    fn decodes_escape_sequences() {
        assert_eq!(
            keys(b"\x1b[A\x1b[B\x1bOC\x1b[D\x1b[H\x1b[4~\x1b[3~\x1bb\x1b\x7f\x1b\x08"),
            [
                plain(Key::Up),
                plain(Key::Down),
                plain(Key::Right),
                plain(Key::Left),
                plain(Key::Home),
                plain(Key::End),
                plain(Key::Delete),
                Some(alt('b')),
                stroke(true, false, Key::Backspace),
                stroke(true, true, Key::Character('h')),
            ]
        );
    }

    #[test]
    fn decodes_modified_cursor_and_function_keys() {
        assert_eq!(
            keys(b"\x1b[1;5C\x1b[1;3D\x1b[1;7A\x1b[1;2B\x1b[5;5~\x1b[Z\x1b[2~\x1b[6~"),
            [
                stroke(false, true, Key::Right),
                stroke(true, false, Key::Left),
                stroke(true, true, Key::Up),
                // Shift isn't something `bind` can express, so it's left out.
                plain(Key::Down),
                stroke(false, true, Key::PageUp),
                plain(Key::BackTab),
                plain(Key::Insert),
                plain(Key::PageDown),
            ]
        );
        assert_eq!(
            keys(b"\x1bOP\x1b[1;5Q\x1b[15~\x1b[24~\x1b[1~\x1b[7~\x1b[8~"),
            [
                plain(Key::F(1)),
                stroke(false, true, Key::F(2)),
                plain(Key::F(5)),
                plain(Key::F(12)),
                plain(Key::Home),
                plain(Key::Home),
                plain(Key::End),
            ]
        );
    }

    #[test]
    fn decodes_every_application_mode_key() {
        assert_eq!(
            keys(b"\x1bOA\x1bOB\x1bOC\x1bOD\x1bOH\x1bOF\x1bOP\x1bOQ\x1bOR\x1bOS"),
            [
                plain(Key::Up),
                plain(Key::Down),
                plain(Key::Right),
                plain(Key::Left),
                plain(Key::Home),
                plain(Key::End),
                plain(Key::F(1)),
                plain(Key::F(2)),
                plain(Key::F(3)),
                plain(Key::F(4)),
            ]
        );
        assert_eq!(
            keys(b"\x1b[1;3R\x1b[1;5S\x1b[1X\x1b[99~"),
            [
                stroke(true, false, Key::F(3)),
                stroke(false, true, Key::F(4)),
                None,
                None
            ]
        );
    }

    #[test]
    fn decodes_escape_then_any_key_as_that_key_with_alt() {
        assert_eq!(keys(b"\x1b\x1b"), [plain(Key::Escape), plain(Key::Escape)]);
        assert_eq!(keys(b"\x1b\x01"), [stroke(true, true, Key::Character('a'))]);
        assert_eq!(
            keys("\x1bé".as_bytes()),
            [stroke(true, false, Key::Character('é'))]
        );
        assert_eq!(keys(b"\x1b\r"), [stroke(true, false, Key::Enter)]);
    }

    #[test]
    fn consumes_unknown_sequences_whole() {
        assert_eq!(keys(b"\x1b[200~x\x1bOZy"), [None, ch('x'), None, ch('y')]);
    }

    #[test]
    fn ends_a_control_sequence_at_a_byte_that_cant_be_in_one() {
        // `ESC [` then Enter is Alt-[ then Enter, not a sequence that swallows Enter
        // and whatever follows.
        assert_eq!(
            keys(b"\x1b[\rls"),
            [Some(alt('[')), plain(Key::Enter), ch('l'), ch('s')]
        );
        assert_eq!(keys(b"\x1bO\x01"), [Some(alt('O')), Some(ctrl('a'))]);
    }

    #[test]
    fn waits_for_the_rest_of_an_escape_sequence_only_while_more_may_come() {
        for (bytes, alone) in [
            (&b"\x1b"[..], plain(Key::Escape)),
            (b"\x1b[", Some(alt('['))),
            (b"\x1b[1;", Some(alt('['))),
            (b"\x1bO", Some(alt('O'))),
        ] {
            assert_eq!(decode(bytes, true), None, "{bytes:?}");
            assert_eq!(
                decode(bytes, false).map(|(key, _)| key),
                Some(alone),
                "{bytes:?}"
            );
        }
    }

    #[test]
    fn replaces_bytes_that_arent_utf8() {
        // A stray continuation byte, a lead byte followed by something else, and a
        // byte that never starts a character.
        let replacement = ch(char::REPLACEMENT_CHARACTER);
        assert_eq!(
            keys(b"\x80a\xe7bc\xff"),
            [
                replacement.clone(),
                ch('a'),
                replacement.clone(),
                ch('b'),
                ch('c'),
                replacement
            ]
        );
        assert_eq!(keys("🦀".as_bytes()), [ch('🦀')]);
    }

    #[test]
    fn replaces_overlong_and_surrogate_encodings() {
        // An overlong `/`, and an encoded surrogate: neither is UTF-8, so each byte
        // is replaced.
        let replaced = keys(b"\xc0\xaf\xed\xa0\x80x");
        assert_eq!(replaced.len(), 6);
        assert!(
            replaced
                .iter()
                .take(5)
                .all(|key| *key == ch(char::REPLACEMENT_CHARACTER))
        );
        assert_eq!(replaced.last(), Some(&ch('x')));
    }

    #[test]
    fn doesnt_hold_back_a_key_after_a_stray_lead_byte() {
        // A lead byte of a 3-byte character, then a plain `a`: the `a` can't
        // continue the character, so it's decoded straight away.
        assert_eq!(
            decode(b"\xe7a", true),
            Some((ch(char::REPLACEMENT_CHARACTER), 1))
        );
        // The start of a character that may yet be completed is held.
        assert_eq!(decode("界".as_bytes().get(..2).unwrap(), true), None);
    }

    #[test]
    fn encodes_keys_as_the_terminal_sends_them() {
        for (stroke, bytes) in [
            (ch('é'), "é".as_bytes()),
            (Some(ctrl('a')), b"\x01"),
            (Some(ctrl('?')), b"\x7f"),
            (Some(alt('b')), b"\x1bb"),
            (stroke(true, true, Key::Character('a')), b"\x1b\x01"),
            (plain(Key::Enter), b"\r"),
            (plain(Key::Tab), b"\t"),
            (plain(Key::Backspace), b"\x7f"),
            (plain(Key::Escape), b"\x1b"),
            (plain(Key::Up), b"\x1b[A"),
            (stroke(false, true, Key::Left), b"\x1b[1;5D"),
            (stroke(true, false, Key::Right), b"\x1b[1;3C"),
            (plain(Key::Home), b"\x1b[H"),
            (plain(Key::End), b"\x1b[F"),
            (plain(Key::Delete), b"\x1b[3~"),
            (stroke(false, true, Key::Delete), b"\x1b[3;5~"),
            (plain(Key::Insert), b"\x1b[2~"),
            (plain(Key::PageUp), b"\x1b[5~"),
            (plain(Key::PageDown), b"\x1b[6~"),
            (plain(Key::BackTab), b"\x1b[Z"),
            (plain(Key::F(1)), b"\x1bOP"),
            (stroke(false, true, Key::F(4)), b"\x1b[1;5S"),
            (plain(Key::F(5)), b"\x1b[15~"),
            (plain(Key::F(12)), b"\x1b[24~"),
            (plain(Key::F(13)), b""),
        ] {
            let stroke = stroke.unwrap();
            assert_eq!(encode(&stroke), bytes, "{stroke:?}");
        }
    }

    #[test]
    fn decodes_what_it_encodes() {
        let named = [
            Key::Up,
            Key::Down,
            Key::Left,
            Key::Right,
            Key::Home,
            Key::End,
            Key::Delete,
            Key::Insert,
            Key::PageUp,
            Key::PageDown,
            Key::F(1),
            Key::F(4),
            Key::F(5),
            Key::F(10),
            Key::F(12),
        ];
        let mut strokes = vec![];
        for key in named {
            for (alt, control) in [(false, false), (true, false), (false, true), (true, true)] {
                strokes.push(KeyStroke {
                    alt,
                    control,
                    shift: false,
                    key: key.clone(),
                });
            }
        }
        strokes.extend([
            ctrl('x'),
            alt('x'),
            KeyStroke {
                alt: true,
                ..ctrl('x')
            },
            KeyStroke {
                alt: true,
                ..Key::Enter.into()
            },
            KeyStroke {
                alt: true,
                ..Key::Backspace.into()
            },
        ]);

        for stroke in strokes {
            assert_eq!(keys(&encode(&stroke)), [Some(stroke.clone())], "{stroke:?}");
        }
    }
}
