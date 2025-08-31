//! String escaping utilities

use std::borrow::Cow;

use itertools::Itertools;

use crate::error;

/// Escape expansion mode.
#[derive(Clone, Copy)]
pub enum EscapeExpansionMode {
    /// echo builtin mode.
    EchoBuiltin,
    /// ANSI-C quotes.
    AnsiCQuotes,
}

/// Expands backslash escapes in the provided string.
///
/// # Arguments
///
/// * `s` - The string to expand.
/// * `mode` - The mode to use for expansion.
#[expect(clippy::too_many_lines)]
pub fn expand_backslash_escapes(
    s: &str,
    mode: EscapeExpansionMode,
) -> Result<(Vec<u8>, bool), error::Error> {
    let mut result: Vec<u8> = vec![];
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            // Not a backslash, add and move on.
            result.append(c.to_string().into_bytes().as_mut());
            continue;
        }

        match it.next() {
            Some('a') => result.push(b'\x07'),
            Some('b') => result.push(b'\x08'),
            Some('c') => {
                match mode {
                    EscapeExpansionMode::EchoBuiltin => {
                        // Stop all additional output!
                        return Ok((result, false));
                    }
                    EscapeExpansionMode::AnsiCQuotes => {
                        if let Some(_next_next) = it.next() {
                            return error::unimp("control character in ANSI C quotes");
                        } else {
                            result.push(b'\\');
                            result.push(b'c');
                        }
                    }
                }
            }
            Some('e' | 'E') => result.push(b'\x1b'),
            Some('f') => result.push(b'\x0c'),
            Some('n') => result.push(b'\n'),
            Some('r') => result.push(b'\r'),
            Some('t') => result.push(b'\t'),
            Some('v') => result.push(b'\x0b'),
            Some('\\') => result.push(b'\\'),
            Some('\'') if matches!(mode, EscapeExpansionMode::AnsiCQuotes) => result.push(b'\''),
            Some('\"') if matches!(mode, EscapeExpansionMode::AnsiCQuotes) => result.push(b'\"'),
            Some('?') if matches!(mode, EscapeExpansionMode::AnsiCQuotes) => result.push(b'?'),
            Some('0') => {
                // Consume 0-3 valid octal chars
                let mut taken_so_far = 0;
                let mut octal_chars: String = it
                    .take_while_ref(|c| {
                        if taken_so_far < 3 && matches!(*c, '0'..='7') {
                            taken_so_far += 1;
                            true
                        } else {
                            false
                        }
                    })
                    .collect();

                if octal_chars.is_empty() {
                    octal_chars.push('0');
                }

                let value = u8::from_str_radix(octal_chars.as_str(), 8)?;
                result.push(value);
            }
            Some('x') => {
                // Consume 1-2 valid hex chars
                let mut taken_so_far = 0;
                let hex_chars: String = it
                    .take_while_ref(|c| {
                        if taken_so_far < 2 && c.is_ascii_hexdigit() {
                            taken_so_far += 1;
                            true
                        } else {
                            false
                        }
                    })
                    .collect();

                if hex_chars.is_empty() {
                    result.push(b'\\');
                    result.append(c.to_string().into_bytes().as_mut());
                } else {
                    let value = u8::from_str_radix(hex_chars.as_str(), 16)?;
                    result.push(value);
                }
            }
            Some('u') => {
                // Consume 1-4 hex digits
                let mut taken_so_far = 0;
                let hex_chars: String = it
                    .take_while_ref(|c| {
                        if taken_so_far < 4 && c.is_ascii_hexdigit() {
                            taken_so_far += 1;
                            true
                        } else {
                            false
                        }
                    })
                    .collect();

                if hex_chars.is_empty() {
                    result.push(b'\\');
                    result.append(c.to_string().into_bytes().as_mut());
                } else {
                    let value = u16::from_str_radix(hex_chars.as_str(), 16)?;

                    if let Some(decoded) = char::from_u32(u32::from(value)) {
                        result.append(decoded.to_string().into_bytes().as_mut());
                    } else {
                        result.push(b'\\');
                        result.append(c.to_string().into_bytes().as_mut());
                    }
                }
            }
            Some('U') => {
                // Consume 1-8 hex digits
                let mut taken_so_far = 0;
                let hex_chars: String = it
                    .take_while_ref(|c| {
                        if taken_so_far < 8 && c.is_ascii_hexdigit() {
                            taken_so_far += 1;
                            true
                        } else {
                            false
                        }
                    })
                    .collect();

                if hex_chars.is_empty() {
                    result.push(b'\\');
                    result.append(c.to_string().into_bytes().as_mut());
                } else {
                    let value = u32::from_str_radix(hex_chars.as_str(), 16)?;

                    if let Some(decoded) = char::from_u32(value) {
                        result.append(decoded.to_string().into_bytes().as_mut());
                    } else {
                        result.push(b'\\');
                        result.append(c.to_string().into_bytes().as_mut());
                    }
                }
            }
            Some(c) => {
                // Not a valid escape sequence.
                result.push(b'\\');
                result.append(c.to_string().into_bytes().as_mut());
            }
            None => {
                // Trailing backslash.
                result.push(b'\\');
            }
        }
    }

    Ok((result, true))
}

/// Quoting mode to use for escaping.
#[derive(Clone, Copy, Default)]
pub enum QuoteMode {
    /// Single-quote.
    #[default]
    SingleQuote,
    /// Double-quote.
    DoubleQuote,
    /// Backslash-escape.
    BackslashEscape,
}

/// Options influencing how to escape/quote an input string.
#[derive(Default)]
pub(crate) struct QuoteOptions {
    /// Whether or not to *always* escape or quote the input; if false, then escaping/quoting
    /// will only be applied if the input contains characters that *require* it.
    pub always_quote: bool,
    /// Preferred mode for quoting/escaping. Quoting may be "upgraded" to a more expressive
    /// format if the input is not expressible otherwise.
    pub preferred_mode: QuoteMode,
    /// Whether or not to *avoid* using ANSI C quoting just for the benefit of newline characters.
    /// Default is for newline characters to require upgrading the string's quoting to
    /// ANSI C quoting.
    pub avoid_ansi_c_quoting_newline: bool,
}

pub(crate) fn quote<'a>(s: &'a str, options: &QuoteOptions) -> Cow<'a, str> {
    let use_ansi_c_quotes = s.contains(|c| {
        needs_ansi_c_quoting(c) && (!options.avoid_ansi_c_quoting_newline || c != '\n')
    });

    if use_ansi_c_quotes {
        return ansi_c_quote(s).into();
    }

    let use_default_quotes =
        !use_ansi_c_quotes && (options.always_quote || s.is_empty() || s.contains(needs_escaping));

    if !use_default_quotes {
        return s.into();
    }

    match options.preferred_mode {
        QuoteMode::BackslashEscape => backslash_escape(s).into(),
        QuoteMode::SingleQuote => single_quote(s).into(),
        QuoteMode::DoubleQuote => double_quote(s).into(),
    }
}

/// Escape the given string, forcing quoting.
///
/// # Arguments
///
/// * `s` - The string to escape.
/// * `mode` - The quoting mode to use.
pub fn force_quote(s: &str, mode: QuoteMode) -> String {
    let options = QuoteOptions {
        always_quote: true,
        preferred_mode: mode,
        ..Default::default()
    };

    quote(s, &options).to_string()
}

/// Applies the given quoting mode to the provided string, only changing it if required.
///
/// # Arguments
///
/// * `s` - The string to escape.
/// * `mode` - The quoting mode to use.
pub fn quote_if_needed(s: &str, mode: QuoteMode) -> Cow<'_, str> {
    let options = QuoteOptions {
        always_quote: false,
        preferred_mode: mode,
        ..Default::default()
    };

    quote(s, &options)
}

fn backslash_escape(s: &str) -> String {
    let mut output = String::new();

    // TODO: Handle other interesting sequences.
    for c in s.chars() {
        match c {
            c if needs_escaping(c) => {
                output.push('\\');
                output.push(c);
            }
            c => output.push(c),
        }
    }

    output
}

fn single_quote(s: &str) -> String {
    // Special-case the empty string.
    if s.is_empty() {
        return "''".into();
    }

    let mut result = String::new();

    // Go through the string; put everything in single quotes except for
    // the single quote character itself. It will get escaped outside
    // all quoting.
    let mut first = true;
    for part in s.split('\'') {
        if !first {
            result.push('\\');
            result.push('\'');
        } else {
            first = false;
        }

        if !part.is_empty() {
            result.push('\'');
            result.push_str(part);
            result.push('\'');
        }
    }

    result
}

fn double_quote(s: &str) -> String {
    let mut result = String::new();

    result.push('"');

    for c in s.chars() {
        if matches!(c, '$' | '`' | '"' | '\\') {
            result.push('\\');
        }

        result.push(c);
    }

    result.push('"');

    result
}

fn ansi_c_quote(s: &str) -> String {
    let mut result = String::new();

    result.push_str("$'");

    for c in s.chars() {
        match c {
            '\x07' => result.push_str("\\a"),
            '\x08' => result.push_str("\\b"),
            '\x1b' => result.push_str("\\E"),
            '\x0c' => result.push_str("\\f"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\x0b' => result.push_str("\\v"),
            '\\' => result.push_str("\\\\"),
            '\'' => result.push_str("\\'"),
            c if needs_ansi_c_quoting(c) => {
                result.push_str(std::format!("\\{:03o}", c as u8).as_str());
            }
            _ => result.push(c),
        }
    }

    result.push('\'');

    result
}

// Returns whether or not the given character needs to be escaped (or quoted) if outside
// quotes.
const fn needs_escaping(c: char) -> bool {
    matches!(
        c,
        '(' | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '$'
            | '*'
            | '?'
            | '|'
            | '&'
            | ';'
            | '<'
            | '>'
            | '`'
            | '\\'
            | '"'
            | '!'
            | '^'
            | ','
            | ' '
            | '\''
    )
}

const fn needs_ansi_c_quoting(c: char) -> bool {
    c.is_ascii_control()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backslash_escape() {
        assert_eq!(quote_if_needed("a", QuoteMode::BackslashEscape), "a");
        assert_eq!(quote_if_needed("a b", QuoteMode::BackslashEscape), r"a\ b");
        assert_eq!(quote_if_needed("", QuoteMode::BackslashEscape), "");
    }

    #[test]
    fn test_single_quote_escape() {
        assert_eq!(quote_if_needed("a", QuoteMode::SingleQuote), "a");
        assert_eq!(quote_if_needed("a b", QuoteMode::SingleQuote), "'a b'");
        assert_eq!(quote_if_needed("", QuoteMode::SingleQuote), "''");
        assert_eq!(quote_if_needed("'", QuoteMode::SingleQuote), "\\'");
    }

    fn assert_echo_expands_to(unexpanded: &str, expected: &str) {
        assert_eq!(
            String::from_utf8(
                expand_backslash_escapes(unexpanded, EscapeExpansionMode::EchoBuiltin)
                    .unwrap()
                    .0
            )
            .unwrap(),
            expected
        );
    }

    #[test]
    fn test_echo_expansion() {
        assert_echo_expands_to("a", "a");
        assert_echo_expands_to(r"\M", "\\M");
        assert_echo_expands_to(r"a\nb", "a\nb");
        assert_echo_expands_to(r"\a", "\x07");
        assert_echo_expands_to(r"\b", "\x08");
        assert_echo_expands_to(r"\e", "\x1b");
        assert_echo_expands_to(r"\f", "\x0c");
        assert_echo_expands_to(r"\n", "\n");
        assert_echo_expands_to(r"\r", "\r");
        assert_echo_expands_to(r"\t", "\t");
        assert_echo_expands_to(r"\v", "\x0b");
        assert_echo_expands_to(r"\\", "\\");
        assert_echo_expands_to(r"\'", "\\'");
        assert_echo_expands_to(r#"\""#, r#"\""#);
        assert_echo_expands_to(r"\?", "\\?");
        assert_echo_expands_to(r"\0", "\0");
        assert_echo_expands_to(r"\00", "\0");
        assert_echo_expands_to(r"\000", "\0");
        assert_echo_expands_to(r"\081", "\081");
        assert_echo_expands_to(r"\0101", "A");
        assert_echo_expands_to(r"abc\", "abc\\");
        assert_echo_expands_to(r"\x41", "A");
        assert_echo_expands_to(r"\xf0\x9f\x90\x8d", "🐍");
        assert_echo_expands_to(r"\u2620", "☠");
        assert_echo_expands_to(r"\U0001f602", "😂");
    }
}
