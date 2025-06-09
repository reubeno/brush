//! Implements a parser for readline binding syntax.

use crate::error;

/// Represents a readline key-sequence binding.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(Eq, serde::Serialize))]
pub struct KeySequenceBinding {
    /// Key sequence to bind
    pub seq: KeySequence,
    /// Command to bind to the sequence
    pub command: String,
}

/// Represents a key sequence.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(Eq, serde::Serialize))]
pub struct KeySequence(pub Vec<KeySequenceItem>);

/// Represents an element of a key sequence.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(Eq, serde::Serialize))]
pub enum KeySequenceItem {
    /// Control
    Control,
    /// Meta
    Meta,
    /// Regular character
    Byte(u8),
}

/// Represents a single key stroke.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(test, derive(Eq, serde::Serialize))]
pub struct KeyStroke {
    /// Meta key is held down
    pub meta: bool,
    /// Control key is held down
    pub control: bool,
    /// Primary key code
    pub key_code: Vec<u8>,
}

/// Parses a key-sequence binding specification.
///
/// # Arguments
///
/// * `input` - The input string to parse
pub fn parse_key_sequence_binding(
    input: &str,
) -> Result<KeySequenceBinding, error::BindingParseError> {
    readline_binding::key_sequence_binding(input)
        .map_err(|_err| error::BindingParseError::Unknown(input.to_owned()))
}

/// Converts a `KeySequence` to a vector of `KeyStroke`.
///
/// # Arguments
///
/// * `seq` - The key sequence to convert
pub fn key_sequence_to_strokes(
    seq: &KeySequence,
) -> Result<Vec<KeyStroke>, error::BindingParseError> {
    let mut strokes = vec![];
    let mut current_stroke = KeyStroke::default();

    for item in &seq.0 {
        if matches!(item, KeySequenceItem::Control | KeySequenceItem::Meta)
            && !current_stroke.key_code.is_empty()
        {
            strokes.push(current_stroke);
            current_stroke = KeyStroke::default();
        }

        match item {
            KeySequenceItem::Control => current_stroke.control = true,
            KeySequenceItem::Meta => current_stroke.meta = true,
            KeySequenceItem::Byte(b) => current_stroke.key_code.push(*b),
        }
    }

    if current_stroke.key_code.is_empty() {
        if current_stroke.control || current_stroke.meta {
            return Err(error::BindingParseError::MissingKeyCode);
        }
    } else {
        strokes.push(current_stroke);
    }

    Ok(strokes)
}

peg::parser! {
    grammar readline_binding() for str {
        rule _() = [' ' | '\t' | '\n']*

        pub rule key_sequence_binding() -> KeySequenceBinding =
            _ "\"" seq:key_sequence() "\"" _ ":" _ command:cmd() _ { KeySequenceBinding { seq, command } }

        rule cmd() -> String = s:$([_]*) { s.to_string() }

        // Main rule for parsing a key sequence
        rule key_sequence() -> KeySequence =
            items:key_sequence_item()* { KeySequence(items) }

        rule key_sequence_item() -> KeySequenceItem =
            "\\C-" { KeySequenceItem::Control } /
            "\\M-" { KeySequenceItem::Meta } /
            "\\e" { KeySequenceItem::Byte(b'\x1b') } /
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
            "\\v" { KeySequenceItem::Byte(b'\x0b') } /
            "\\" n:octal_number() { KeySequenceItem::Byte(n) } /
            "\\" n:hex_number() { KeySequenceItem::Byte(n) } /
            [c if c != '"'] { KeySequenceItem::Byte(c as u8) }

        rule octal_number() -> u8 =
            s:$(['0'..='7']*<1,3>) {? u8::from_str_radix(s, 8).or(Err("invalid octal number")) }

        rule hex_number() -> u8 =
            s:$(['0'..='9' | 'a'..='f' | 'A'..='F']*<1,2>) {? u8::from_str_radix(s, 16).or(Err("invalid hex number")) }
    }
}

#[cfg(test)]
#[allow(clippy::unnecessary_wraps)]
#[allow(clippy::panic_in_result_fn)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_basic_parse() -> Result<()> {
        let binding = parse_key_sequence_binding(r#""\C-k": xyz"#)?;
        assert_eq!(
            binding.seq.0,
            [KeySequenceItem::Control, KeySequenceItem::Byte(b'k')]
        );
        assert_eq!(binding.command, "xyz");

        Ok(())
    }
}
