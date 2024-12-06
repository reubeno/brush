use std::borrow::Cow;

use itertools::Itertools;

use crate::error;

#[derive(Clone, Copy)]
pub(crate) enum EscapeExpansionMode {
    EchoBuiltin,
    AnsiCQuotes,
}

#[allow(clippy::too_many_lines)]
pub(crate) fn expand_backslash_escapes(
    s: &str,
    mode: EscapeExpansionMode,
) -> Result<(Vec<u8>, bool), crate::error::Error> {
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

#[derive(Clone, Copy)]
pub(crate) enum QuoteMode {
    BackslashEscape,
    Quote,
}

pub(crate) fn force_quote(s: &str, mode: QuoteMode) -> String {
    match mode {
        QuoteMode::BackslashEscape => escape_with_backslash(s, true).to_string(),
        QuoteMode::Quote => escape_with_quoting(s, true).to_string(),
    }
}

pub(crate) fn quote_if_needed(s: &str, mode: QuoteMode) -> Cow<'_, str> {
    match mode {
        QuoteMode::BackslashEscape => escape_with_backslash(s, false),
        QuoteMode::Quote => escape_with_quoting(s, false),
    }
}

fn escape_with_backslash(s: &str, force: bool) -> Cow<'_, str> {
    if !force && !s.chars().any(needs_escaping) {
        return s.into();
    }

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

    output.into()
}

fn escape_with_quoting(s: &str, force: bool) -> Cow<'_, str> {
    // TODO: Handle single-quote!
    if force || s.is_empty() || s.chars().any(needs_escaping) {
        std::format!("'{s}'").into()
    } else {
        s.into()
    }
}

fn needs_escaping(c: char) -> bool {
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
    )
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
    fn test_quote_escape() {
        assert_eq!(quote_if_needed("a", QuoteMode::Quote), "a");
        assert_eq!(quote_if_needed("a b", QuoteMode::Quote), "'a b'");
        assert_eq!(quote_if_needed("", QuoteMode::Quote), "''");
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
