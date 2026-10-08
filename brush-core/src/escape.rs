//! String escaping utilities

use std::borrow::Cow;

use itertools::Itertools;

use crate::{error, int_utils};

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
    let mut result: Vec<u8> = Vec::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            // Not a backslash, add and move on.
            result.append(c.to_string().into_bytes().as_mut());
            continue;
        }

        let Some(escape_cmd) = it.next() else {
            // Trailing backslash.
            result.push(b'\\');
            continue;
        };

        match escape_cmd {
            'a' => result.push(b'\x07'),
            'b' => result.push(b'\x08'),
            'c' => {
                match mode {
                    EscapeExpansionMode::EchoBuiltin => {
                        // Stop all additional output!
                        return Ok((result, false));
                    }
                    EscapeExpansionMode::AnsiCQuotes => {
                        if let Some(char_value) = it.next() {
                            // Special case backslash. If it's immediately followed by another
                            // backslash, then we consume both; if not, we still will use the
                            // backslash character as the one to apply the control transformation
                            // to.
                            if char_value == '\\' {
                                let orig_it = it.clone();
                                if !matches!(it.next(), Some('\\')) {
                                    // Didn't find another backslash; restore iterator.
                                    it = orig_it;
                                }
                            }

                            let mut bytes: Vec<u8> = if char_value.is_ascii_lowercase() {
                                char_value
                                    .to_ascii_uppercase()
                                    .to_string()
                                    .bytes()
                                    .collect()
                            } else {
                                char_value.to_string().bytes().collect()
                            };

                            if !bytes.is_empty() {
                                if bytes[0] == b'?' {
                                    // We can't explain why this is the case, but it is.
                                    bytes[0] = 0x7f;
                                } else {
                                    bytes[0] &= 0x1f;
                                }
                            }

                            result.append(bytes.as_mut());
                        } else {
                            result.push(b'\\');
                            result.push(b'c');
                        }
                    }
                }
            }
            'e' | 'E' => result.push(b'\x1b'),
            'f' => result.push(b'\x0c'),
            'n' => result.push(b'\n'),
            'r' => result.push(b'\r'),
            't' => result.push(b'\t'),
            'v' => result.push(b'\x0b'),
            '\\' => result.push(b'\\'),
            '\'' if matches!(mode, EscapeExpansionMode::AnsiCQuotes) => result.push(b'\''),
            '\"' if matches!(mode, EscapeExpansionMode::AnsiCQuotes) => result.push(b'\"'),
            '?' if matches!(mode, EscapeExpansionMode::AnsiCQuotes) => result.push(b'?'),
            '0' => {
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

                let value = int_utils::parse::<u8>(octal_chars.as_str(), 8)?;
                result.push(value);
            }
            'x' => {
                // Consume 1-2 valid hex chars (or unlimited with braces in ANSI-C mode)
                let mut hex_chars = String::new();
                let mut invalid_prefix = false;
                let mut hexits_consumed = 0;
                let mut start_brace_consumed = false;

                loop {
                    // Save the original in case we go too far and need to restore.
                    let orig_it = it.clone();

                    let Some(next_c) = it.next() else {
                        break;
                    };

                    if matches!(mode, EscapeExpansionMode::AnsiCQuotes)
                        && !start_brace_consumed
                        && next_c == '{'
                    {
                        start_brace_consumed = true;
                    } else if start_brace_consumed && next_c == '}' {
                        break;
                    } else if ((start_brace_consumed && !invalid_prefix)
                        || (!start_brace_consumed && hexits_consumed < 2))
                        && next_c.is_ascii_hexdigit()
                    {
                        hex_chars.push(next_c);
                        hexits_consumed += 1;
                    } else if start_brace_consumed && hexits_consumed == 0 {
                        invalid_prefix = true;
                    } else {
                        // Went too far; restore iterator and break.
                        it = orig_it;
                        break;
                    }
                }

                if hex_chars.is_empty() {
                    if start_brace_consumed {
                        result.push(0);
                    } else {
                        result.push(b'\\');
                        result.append(escape_cmd.to_string().into_bytes().as_mut());
                    }
                } else {
                    let value32 = int_utils::parse::<u32>(hex_chars.as_str(), 16)?;
                    let value8: u8 = (value32 & 0xFF) as u8;
                    result.push(value8);
                }
            }
            'u' => {
                // Consume 1-4 hex digits
                let mut taken_so_far = 0;
                let hex_chars: String = it
                    .take_while_ref(|next_c| {
                        if taken_so_far < 4 && next_c.is_ascii_hexdigit() {
                            taken_so_far += 1;
                            true
                        } else {
                            false
                        }
                    })
                    .collect();

                if hex_chars.is_empty() {
                    result.push(b'\\');
                    result.append(escape_cmd.to_string().into_bytes().as_mut());
                } else {
                    let value = int_utils::parse::<u16>(hex_chars.as_str(), 16)?;
                    if let Some(decoded) = char::from_u32(u32::from(value)) {
                        result.append(decoded.to_string().into_bytes().as_mut());
                    } else {
                        result.push(b'\\');
                        result.append(escape_cmd.to_string().into_bytes().as_mut());
                    }
                }
            }
            'U' => {
                // Consume 1-8 hex digits
                let mut taken_so_far = 0;
                let hex_chars: String = it
                    .take_while_ref(|next_c| {
                        if taken_so_far < 8 && next_c.is_ascii_hexdigit() {
                            taken_so_far += 1;
                            true
                        } else {
                            false
                        }
                    })
                    .collect();

                if hex_chars.is_empty() {
                    result.push(b'\\');
                    result.append(escape_cmd.to_string().into_bytes().as_mut());
                } else {
                    let value = int_utils::parse::<u32>(hex_chars.as_str(), 16)?;
                    if let Some(decoded) = char::from_u32(value) {
                        result.append(decoded.to_string().into_bytes().as_mut());
                    } else {
                        result.push(b'\\');
                        result.append(escape_cmd.to_string().into_bytes().as_mut());
                    }
                }
            }
            first_octal @ '1'..='7' if matches!(mode, EscapeExpansionMode::AnsiCQuotes) => {
                // We've already consumed the first octal digit.
                let mut octal_chars = String::new();
                octal_chars.push(first_octal);

                // Consume up to 2 more valid octal chars
                let mut taken_so_far = 1;
                for next_c in it.take_while_ref(|next_c| {
                    if taken_so_far < 3 && matches!(next_c, '0'..='7') {
                        taken_so_far += 1;
                        true
                    } else {
                        false
                    }
                }) {
                    octal_chars.push(next_c);
                }

                let value = int_utils::parse::<u8>(octal_chars.as_str(), 8)?;
                result.push(value);
            }
            unknown => {
                // Not a valid escape sequence.
                result.push(b'\\');
                result.append(unknown.to_string().into_bytes().as_mut());
            }
        }
    }

    // In ANSI-C quotes, we crop the result at the first NUL.
    if matches!(mode, EscapeExpansionMode::AnsiCQuotes) {
        if let Some(nul_index) = result.iter().position(|&b| b == 0) {
            result.truncate(nul_index);
        }
    }

    Ok((result, true))
}

/// Quoting mode to use for escaping.
#[derive(Clone, Copy, Debug, Default)]
pub enum QuoteMode {
    /// Single-quote.
    #[default]
    SingleQuote,
    /// Double-quote.
    DoubleQuote,
    /// Backslash-escape.
    BackslashEscape,
    /// ANSI-C quote (`$'...'`).
    AnsiC,
}

/// Options for [`quote`]: how to quote a string so that, read back as a shell word, it
/// yields just that string, and which chars that are special only in some places (or to
/// some readers) to treat as special.
///
/// Build one with [`QuoteOptions::builder`]; every option defaults to off (and the mode to
/// [`QuoteMode::SingleQuote`]), which is what [`quote_if_needed`] uses.
#[derive(Clone, Debug, Default, bon::Builder)]
pub struct QuoteOptions {
    /// Whether to quote the input even if nothing in it needs quoting.
    #[builder(default)]
    pub(crate) always_quote: bool,
    /// Preferred mode for quoting. It's upgraded to ANSI-C quoting (`$'...'`) if the input
    /// has chars only that can express.
    #[builder(default)]
    pub(crate) preferred_mode: QuoteMode,
    /// Whether to avoid ANSI-C quoting just for newlines, which otherwise require it.
    #[builder(default)]
    pub(crate) avoid_ansi_c_quoting_newline: bool,
    /// Whether to leave a `#` starting the input unquoted. Bash does in some output that
    /// isn't read back as shell input, e.g. tracing a `[[ -n ... ]]` test.
    #[builder(default)]
    pub(crate) leave_leading_hash: bool,
    /// Whether to leave a `~` unquoted where it would be tilde-expanded. Bash does when
    /// quoting a file name it completes, so that a `~user` completion still expands.
    #[builder(default)]
    pub(crate) leave_tilde: bool,
    /// Whether to quote commas, which aren't special on their own. Bash's `printf %q`
    /// does, so its output is safe even inside a brace expansion.
    #[builder(default)]
    pub(crate) quote_commas: bool,
}

/// Quotes `s` as `options` say, so that, read back as a shell word, it yields just `s`.
///
/// # Arguments
///
/// * `s` - The string to quote.
/// * `options` - How to quote it.
pub fn quote<'a>(s: &'a str, options: &QuoteOptions) -> Cow<'a, str> {
    let use_ansi_c_quotes = s.contains(|c| {
        needs_ansi_c_quoting(c) && (!options.avoid_ansi_c_quoting_newline || c != '\n')
    });

    if use_ansi_c_quotes {
        return ansi_c_quote(s).into();
    }

    let use_default_quotes =
        !use_ansi_c_quotes && (options.always_quote || s.is_empty() || needs_quoting(s, options));

    if !use_default_quotes {
        return s.into();
    }

    match options.preferred_mode {
        QuoteMode::BackslashEscape => backslash_escape(s, options),
        QuoteMode::SingleQuote => single_quote(s),
        QuoteMode::DoubleQuote => double_quote(s).into(),
        QuoteMode::AnsiC => ansi_c_quote(s).into(),
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

fn backslash_escape<'a>(s: &'a str, options: &QuoteOptions) -> Cow<'a, str> {
    if s.is_empty() {
        // An empty string must be represented as '' to be a valid shell word.
        Cow::Owned("''".to_string())
    } else if !needs_quoting(s, options) {
        Cow::Borrowed(s)
    } else {
        let mut output = String::with_capacity(s.len());
        for (i, c) in s.char_indices() {
            if needs_escaping_at(s, i, c, options) {
                output.push('\\');
            }
            output.push(c);
        }
        Cow::Owned(output)
    }
}

/// Plain single-quoting, as bash's `sh_single_quote` does it.
///
/// Unlike [`force_quote`] and [`quote_if_needed`], this never upgrades to ANSI-C quoting;
/// control characters are emitted literally. Use it where bash prints values verbatim,
/// such as `alias` output.
///
/// # Arguments
///
/// * `s` - The string to quote.
pub fn single_quote(s: &str) -> Cow<'_, str> {
    // Special-case the empty string and a lone single quote; both are rendered
    // without any empty quoted runs.
    match s {
        "" => return Cow::Borrowed("''"),
        "'" => return Cow::Borrowed(r"\'"),
        _ => (),
    }

    let mut result = String::with_capacity(s.len() + 2);

    // Go through the string; put everything in single quotes except for
    // the single quote character itself. It will get escaped outside
    // all quoting, with the quoted runs on either side of it kept even
    // when they're empty.
    for (i, part) in s.split('\'').enumerate() {
        if i > 0 {
            result.push_str(r"\'");
        }

        result.push('\'');
        result.push_str(part);
        result.push('\'');
    }

    Cow::Owned(result)
}

fn double_quote(s: &str) -> String {
    double_quote_leaving(s, |_, _| false)
}

/// Double-quotes `s`, escaping the chars that are special in double quotes (`$`, `` ` ``,
/// `"`, and `\`) -- except those `live` says to leave live, given each one's byte offset in
/// `s` and the char. E.g., with every `$` live, parameters in `s` still expand.
pub(crate) fn double_quote_leaving(s: &str, live: impl Fn(usize, char) -> bool) -> String {
    let mut result = String::with_capacity(s.len() + 2);

    result.push('"');

    for (index, c) in s.char_indices() {
        if matches!(c, '$' | '`' | '"' | '\\') && !live(index, c) {
            result.push('\\');
        }

        result.push(c);
    }

    result.push('"');

    result
}

fn ansi_c_quote(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
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

/// Returns whether `s` has a char that needs quoting for `s` to be a shell word that
/// expands to just `s`, as `options` say.
fn needs_quoting(s: &str, options: &QuoteOptions) -> bool {
    s.char_indices()
        .any(|(i, c)| needs_escaping_at(s, i, c, options))
}

/// Returns whether `c`, at byte offset `i` in `s`, needs quoting: if it's always special,
/// or, like bash, it's a `#` starting the word (which would start a comment), or a `~`
/// starting it or following a `=` or `:` (where it would be tilde-expanded) -- unless
/// `options` say to leave those.
fn needs_escaping_at(s: &str, i: usize, c: char, options: &QuoteOptions) -> bool {
    match c {
        '#' => i == 0 && !options.leave_leading_hash,
        '~' if options.leave_tilde => false,
        ',' => options.quote_commas,
        '~' => {
            let prev = s.get(..i).and_then(|before| before.chars().next_back());
            matches!(prev, None | Some('=' | ':'))
        }
        _ => needs_escaping(c),
    }
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
        assert_eq!(quote_if_needed("", QuoteMode::BackslashEscape), "''");
    }

    #[test]
    fn test_leading_hash_and_tilde_need_quoting() {
        // Like bash: a `#` starting a word, and a `~` starting it or following `=` or `:`.
        assert_eq!(quote_if_needed("#a", QuoteMode::BackslashEscape), r"\#a");
        assert_eq!(quote_if_needed("~/x", QuoteMode::BackslashEscape), r"\~/x");
        assert_eq!(quote_if_needed("a=~", QuoteMode::BackslashEscape), r"a=\~");
        assert_eq!(quote_if_needed("a:~", QuoteMode::BackslashEscape), r"a:\~");
        assert_eq!(quote_if_needed("#a", QuoteMode::SingleQuote), "'#a'");
        assert_eq!(quote_if_needed("~", QuoteMode::SingleQuote), "'~'");
        // Elsewhere, they're ordinary chars.
        assert_eq!(quote_if_needed("a#", QuoteMode::SingleQuote), "a#");
        assert_eq!(quote_if_needed("a~", QuoteMode::SingleQuote), "a~");
        // A multibyte char before them doesn't change that.
        assert_eq!(quote_if_needed("é=~", QuoteMode::BackslashEscape), r"é=\~");
        assert_eq!(quote_if_needed("é~", QuoteMode::BackslashEscape), "é~");
        assert_eq!(quote_if_needed("é#", QuoteMode::BackslashEscape), "é#");
    }

    #[test]
    fn test_single_quote_escape() {
        assert_eq!(quote_if_needed("a", QuoteMode::SingleQuote), "a");
        assert_eq!(quote_if_needed("a b", QuoteMode::SingleQuote), "'a b'");
        assert_eq!(quote_if_needed("", QuoteMode::SingleQuote), "''");
        assert_eq!(quote_if_needed("'", QuoteMode::SingleQuote), r"\'");

        // The quoted run on either side of an escaped quote is kept even when it's empty;
        // only a string that is *nothing but* a single quote loses its surrounding quotes.
        assert_eq!(quote_if_needed("a'b", QuoteMode::SingleQuote), r"'a'\''b'");
        assert_eq!(
            quote_if_needed("a''b", QuoteMode::SingleQuote),
            r"'a'\'''\''b'"
        );
        assert_eq!(quote_if_needed("'a", QuoteMode::SingleQuote), r"''\''a'");
        assert_eq!(quote_if_needed("a'", QuoteMode::SingleQuote), r"'a'\'''");
        assert_eq!(quote_if_needed("''", QuoteMode::SingleQuote), r"''\'''\'''");
    }

    #[test]
    fn test_single_quote_never_upgrades() {
        // Unlike `quote`, this never reaches for ANSI-C quoting; control characters are
        // emitted literally.
        assert_eq!(single_quote("a\tb"), "'a\tb'");
        assert_eq!(single_quote("a\nb"), "'a\nb'");
        assert_eq!(single_quote("a"), "'a'");

        // ...whereas `quote` does upgrade for the same input.
        assert_eq!(quote_if_needed("a\tb", QuoteMode::SingleQuote), r"$'a\tb'");
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
