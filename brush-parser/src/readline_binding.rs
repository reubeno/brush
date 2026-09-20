//! Implements a parser for readline binding syntax.

use crate::error;

/// Represents a key-sequence-to-shell-command binding.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize))]
pub struct KeySequenceShellCommandBinding {
    /// Key sequence to bind
    pub seq: KeySequence,
    /// Shell command to bind to the sequence
    pub shell_cmd: String,
}

/// Represents a key-sequence-to-readline-command binding.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize))]
pub struct KeySequenceReadlineBinding {
    /// Key sequence to bind
    pub seq: KeySequence,
    /// Readline target to bind to the sequence
    pub target: ReadlineTarget,
}

/// Represents a readline target.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize))]
pub enum ReadlineTarget {
    /// A named readline function.
    Function(String),
    /// A readline command macro.
    Macro(String),
}

/// Represents a key sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize))]
pub struct KeySequence(pub Vec<KeySequenceItem>);

/// Represents an element of a key sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(serde::Serialize, serde::Deserialize))]
pub enum KeySequenceItem {
    /// Control
    Control,
    /// Meta
    Meta,
    /// A literal UTF-8 byte or a byte produced by an escape.
    Byte(u8),
}

/// Parses a key sequence.
///
/// # Arguments
///
/// * `input` - The input string to parse
pub fn parse_key_sequence(input: &str) -> Result<KeySequence, error::BindingParseError> {
    readline_binding::key_sequence(input)
        .map_err(|_err| error::BindingParseError::Unknown(input.to_owned()))
}

/// Parses a binding specification that maps a key sequence
/// to a shell command.
///
/// # Arguments
///
/// * `input` - The input string to parse
pub fn parse_key_sequence_shell_cmd_binding(
    input: &str,
) -> Result<KeySequenceShellCommandBinding, error::BindingParseError> {
    readline_binding::key_sequence_shell_cmd_binding(input)
        .map_err(|_err| error::BindingParseError::Unknown(input.to_owned()))
}

/// Parses a binding specification that maps a key sequence
/// to a readline target.
///
/// # Arguments
///
/// * `input` - The input string to parse
pub fn parse_key_sequence_readline_binding(
    input: &str,
) -> Result<KeySequenceReadlineBinding, error::BindingParseError> {
    readline_binding::key_sequence_readline_binding(input)
        .map_err(|_err| error::BindingParseError::Unknown(input.to_owned()))
}

/// Returns the byte produced by holding Control while pressing the key for `byte`.
///
/// Follows readline: `\C-?` is DEL (0x7f); every other key is masked to its low five bits,
/// so upper- and lower-case letters produce the same control code.
#[must_use]
pub const fn control_byte(byte: u8) -> u8 {
    if byte == b'?' { 0x7f } else { byte & 0x1f }
}

/// Expands a parsed binding trigger into terminal bytes.
///
/// Control prefixes transform the following byte into its ASCII control-code equivalent.
/// Meta prefixes are represented using an escape-byte prefix. Macro bodies instead retain
/// the high bit set by meta notation; see [`macro_sequence_to_bytes`].
///
/// # Arguments
///
/// * `seq` - The key sequence to expand
pub fn key_sequence_to_bytes(seq: &KeySequence) -> Result<Vec<u8>, error::BindingParseError> {
    sequence_to_bytes(seq, MetaEncoding::EscapePrefix)
}

/// Expands a parsed macro body into the bytes stored and replayed by readline.
///
/// Unlike binding triggers, `\M-` in a macro body sets the high bit of the following byte.
/// This preserves raw bytes when reading the meta notation emitted by `bind -s`. Use `\e`
/// for an explicit escape prefix.
///
/// # Arguments
///
/// * `seq` - The macro body to expand.
pub fn macro_sequence_to_bytes(seq: &KeySequence) -> Result<Vec<u8>, error::BindingParseError> {
    sequence_to_bytes(seq, MetaEncoding::HighBit)
}

#[derive(Clone, Copy)]
enum MetaEncoding {
    EscapePrefix,
    HighBit,
}

fn sequence_to_bytes(
    seq: &KeySequence,
    meta_encoding: MetaEncoding,
) -> Result<Vec<u8>, error::BindingParseError> {
    let mut bytes = Vec::new();
    let mut control = false;
    let mut meta = false;

    for item in &seq.0 {
        match item {
            KeySequenceItem::Control => control = true,
            KeySequenceItem::Meta => meta = true,
            KeySequenceItem::Byte(byte) => {
                let mut byte = if control { control_byte(*byte) } else { *byte };
                if meta {
                    match meta_encoding {
                        MetaEncoding::EscapePrefix => bytes.push(b'\x1b'),
                        MetaEncoding::HighBit => byte |= 0x80,
                    }
                }

                bytes.push(byte);
                control = false;
                meta = false;
            }
        }
    }

    if control || meta {
        Err(error::BindingParseError::MissingKeyCode)
    } else {
        Ok(bytes)
    }
}

peg::parser! {
    grammar readline_binding() for str {
        rule _() = [' ' | '\t' | '\n']*

        pub rule key_sequence_shell_cmd_binding() -> KeySequenceShellCommandBinding =
            _ "\"" seq:key_sequence() "\"" _ ":" _ cmd:shell_cmd() _ { KeySequenceShellCommandBinding { seq, shell_cmd: cmd } }

        pub rule key_sequence_readline_binding() -> KeySequenceReadlineBinding =
            _ "\"" seq:key_sequence() "\"" _ ":" _ "\"" cmd:readline_cmd() "\"" _ {
                KeySequenceReadlineBinding { seq, target: ReadlineTarget::Macro(cmd) }
            } /
            _ "\"" seq:key_sequence() "\"" _ ":" _ func:readline_function() _ {
                KeySequenceReadlineBinding { seq, target: ReadlineTarget::Function(func) }
            }

        rule readline_cmd() -> String = s:$(("\\" [_] / [^'"'])*) { s.to_string() }
        rule shell_cmd() -> String = s:$([_]*) { s.to_string() }
        rule readline_function() -> String = s:$([_]*) { s.to_string() }

        // Main rule for parsing a key sequence
        pub rule key_sequence() -> KeySequence =
            parts:(
                item:key_sequence_item() { vec![item] } /
                // readline drops the backslash of an escape it does not recognize and
                // keeps the character, so `\z` is `z` and `\C` (no `-`) is `C`.
                "\\" escaped:$([_]) {
                    escaped.bytes().map(KeySequenceItem::Byte).collect::<Vec<_>>()
                } /
                literal:$([^'"' | '\\']+ / "\\") {
                    literal.bytes().map(KeySequenceItem::Byte).collect::<Vec<_>>()
                }
            )* { KeySequence(parts.into_iter().flatten().collect()) }

        rule key_sequence_item() -> KeySequenceItem =
            "\\C-" { KeySequenceItem::Control } /
            "\\M-" { KeySequenceItem::Meta } /
            "\\e" { KeySequenceItem::Byte(b'\x1b') } /
            "\\x" n:hex_number() { KeySequenceItem::Byte(n) } /
            "\\" n:octal_number() { KeySequenceItem::Byte(n) } /
            "\\\\" { KeySequenceItem::Byte(b'\\') } /
            "\\\"" { KeySequenceItem::Byte(b'"') } /
            "\\'" { KeySequenceItem::Byte(b'\'') } /
            "\\a" { KeySequenceItem::Byte(b'\x07') } /
            "\\b" { KeySequenceItem::Byte(b'\x08') } /
            "\\d" { KeySequenceItem::Byte(b'\x7f') } /
            "\\f" { KeySequenceItem::Byte(b'\x0c') } /
            "\\n" { KeySequenceItem::Byte(b'\n') } /
            "\\r" { KeySequenceItem::Byte(b'\r') } /
            "\\t" { KeySequenceItem::Byte(b'\t') } /
            "\\v" { KeySequenceItem::Byte(b'\x0b') }

        rule octal_number() -> u8 =
            // Up to 0o777; readline keeps the low byte instead of rejecting the value.
            s:$(['0'..='7']*<1,3>) {? u16::from_str_radix(s, 8).map(|n| (n & 0xff) as u8).or(Err("invalid octal number")) }

        rule hex_number() -> u8 =
            s:$(['0'..='9' | 'a'..='f' | 'A'..='F']*<1,2>) {? u8::from_str_radix(s, 16).or(Err("invalid hex number")) }
    }
}

#[cfg(test)]
#[expect(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_basic_shell_cmd_binding_parse() -> Result<()> {
        let binding = parse_key_sequence_shell_cmd_binding(r#""\C-k": xyz"#)?;
        assert_eq!(
            binding.seq.0,
            [KeySequenceItem::Control, KeySequenceItem::Byte(b'k')]
        );
        assert_eq!(binding.shell_cmd, "xyz");

        Ok(())
    }

    #[test]
    fn test_octal_escapes_above_a_byte_are_masked() -> Result<()> {
        // readline keeps the low byte of an octal escape rather than rejecting it.
        let binding = parse_key_sequence_shell_cmd_binding(r#""\401\777\0x": xyz"#)?;
        assert_eq!(
            binding.seq.0,
            [
                KeySequenceItem::Byte(0x01),
                KeySequenceItem::Byte(0xff),
                KeySequenceItem::Byte(0x00),
                KeySequenceItem::Byte(b'x'),
            ]
        );

        Ok(())
    }

    #[test]
    fn test_basic_readline_func_binding_parse() -> Result<()> {
        let binding = parse_key_sequence_readline_binding(r#""\M-x": some-function"#)?;
        assert_eq!(
            binding.seq.0,
            [KeySequenceItem::Meta, KeySequenceItem::Byte(b'x')]
        );
        assert_eq!(
            binding.target,
            ReadlineTarget::Function("some-function".to_string())
        );

        Ok(())
    }

    #[test]
    fn test_basic_readline_cmd_binding_parse() -> Result<()> {
        let binding = parse_key_sequence_readline_binding(r#""\C-k": "xyz""#)?;
        assert_eq!(
            binding.seq.0,
            [KeySequenceItem::Control, KeySequenceItem::Byte(b'k')]
        );
        assert_eq!(binding.target, ReadlineTarget::Macro(String::from("xyz")));

        Ok(())
    }

    #[test]
    fn test_key_sequence_to_terminal_bytes() -> Result<()> {
        let sequence = parse_key_sequence(r"text\C-a\M-f\e\x7f")?;

        assert_eq!(key_sequence_to_bytes(&sequence)?, b"text\x01\x1bf\x1b\x7f");

        Ok(())
    }

    #[test]
    fn test_macro_body_may_contain_escaped_quotes() -> Result<()> {
        let binding = parse_key_sequence_readline_binding(r#""\C-g": "a\"b\\""#)?;
        assert_eq!(
            binding.target,
            ReadlineTarget::Macro(r#"a\"b\\"#.to_owned())
        );

        let body = parse_key_sequence(r#"a\"b\\"#)?;
        assert_eq!(key_sequence_to_bytes(&body)?, b"a\"b\\");

        Ok(())
    }

    #[test]
    fn test_control_byte_follows_readline() {
        assert_eq!(control_byte(b'?'), 0x7f);
        assert_eq!(control_byte(b'a'), 0x01);
        assert_eq!(control_byte(b'A'), 0x01);
        assert_eq!(control_byte(b'@'), 0x00);
        assert_eq!(control_byte(b'_'), 0x1f);
    }

    #[test]
    fn test_macro_bytes_control_question_is_del() -> Result<()> {
        let sequence = parse_key_sequence(r"a\C-?b")?;
        assert_eq!(key_sequence_to_bytes(&sequence)?, b"a\x7fb");

        Ok(())
    }

    #[test]
    fn test_macro_bytes_meta_control_combination() -> Result<()> {
        let sequence = parse_key_sequence(r"\M-\C-x")?;
        assert_eq!(key_sequence_to_bytes(&sequence)?, b"\x1b\x18");
        assert_eq!(macro_sequence_to_bytes(&sequence)?, b"\x98");

        let sequence = parse_key_sequence(r"\C-\M-x")?;
        assert_eq!(macro_sequence_to_bytes(&sequence)?, b"\x98");

        Ok(())
    }

    #[test]
    fn test_macro_meta_notation_preserves_high_bytes() -> Result<()> {
        let sequence = parse_key_sequence(r"\M-C\M-)\M-\C-@\M-\C-?\ef")?;
        assert_eq!(
            macro_sequence_to_bytes(&sequence)?,
            b"\xc3\xa9\x80\xff\x1bf"
        );

        Ok(())
    }

    #[test]
    fn test_literal_unicode_uses_utf8_bytes() -> Result<()> {
        let text = "\u{e9}\u{1f600}\u{10d}";
        let sequence = parse_key_sequence(text)?;
        assert_eq!(key_sequence_to_bytes(&sequence)?, text.as_bytes());
        assert_eq!(macro_sequence_to_bytes(&sequence)?, text.as_bytes());

        Ok(())
    }

    #[test]
    fn test_macro_bytes_numeric_escapes() -> Result<()> {
        let sequence = parse_key_sequence(r"\177\x7f\d\0")?;
        assert_eq!(key_sequence_to_bytes(&sequence)?, b"\x7f\x7f\x7f\x00");

        Ok(())
    }

    #[test]
    fn test_hex_escape_parses_as_single_byte() -> Result<()> {
        let sequence = parse_key_sequence(r"\x41\x7F")?;
        assert_eq!(
            sequence.0,
            [KeySequenceItem::Byte(0x41), KeySequenceItem::Byte(0x7f)]
        );

        Ok(())
    }

    #[test]
    fn test_unknown_escape_keeps_only_the_character() -> Result<()> {
        // Checked against bash: readline's key-sequence translator drops the backslash of
        // an escape it does not recognize. `\C`, `\M` and `\x` are unknown escapes too,
        // since only `\C-`, `\M-` and `\x` followed by hex digits are known.
        for (input, expected) in [
            (r"a\zb", &b"azb"[..]),
            (r"\C\M\x", b"CMx"),
            (r"\z", b"z"),
            (r"\-", b"-"),
            (r"\C-\z", b"\x1a"),
            (r"\M-\z", b"\x1bz"),
        ] {
            let sequence = parse_key_sequence(input)?;
            assert_eq!(key_sequence_to_bytes(&sequence)?, expected, "{input:?}");
        }

        // A known escape still wins over the fallthrough.
        assert_eq!(
            key_sequence_to_bytes(&parse_key_sequence(r#"\e\\\"a"#)?)?,
            b"\x1b\\\"a"
        );
        assert_eq!(
            key_sequence_to_bytes(&parse_key_sequence(r"\x41\101")?)?,
            b"AA"
        );

        // The escaped character is kept whole, however many bytes it takes.
        assert_eq!(
            key_sequence_to_bytes(&parse_key_sequence("\\\u{e9}\\\u{1f600}")?)?,
            "\u{e9}\u{1f600}".as_bytes()
        );

        // A lone trailing backslash has nothing to escape and stays as it is.
        assert_eq!(key_sequence_to_bytes(&parse_key_sequence("a\\")?)?, b"a\\");

        Ok(())
    }

    #[test]
    fn test_macro_bytes_reject_trailing_modifier() -> Result<()> {
        for input in [r"\C-", r"\M-", r"a\C-"] {
            let sequence = parse_key_sequence(input)?;
            assert!(
                key_sequence_to_bytes(&sequence).is_err(),
                "expected error for {input:?}"
            );
            assert!(
                macro_sequence_to_bytes(&sequence).is_err(),
                "expected macro error for {input:?}"
            );
        }

        Ok(())
    }
}
