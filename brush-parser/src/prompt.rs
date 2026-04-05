//! Parser for shell prompt syntax (e.g., `PS1`).

use crate::error;
use crate::parser::ParserImpl;

/// A piece of a prompt string.
#[derive(Clone, Debug)]
#[cfg_attr(
    any(test, feature = "serde"),
    derive(PartialEq, Eq, serde::Serialize, serde::Deserialize)
)]
pub enum PromptPiece {
    /// An ASCII character.
    AsciiCharacter(u32),
    /// A backslash character.
    Backslash,
    /// The bell character.
    BellCharacter,
    /// A carriage return character.
    CarriageReturn,
    /// The current command number.
    CurrentCommandNumber,
    /// The current history number.
    CurrentHistoryNumber,
    /// The name of the current user.
    CurrentUser,
    /// Path to the current working directory.
    CurrentWorkingDirectory {
        /// Whether or not to apply tilde-replacement before expanding.
        tilde_replaced: bool,
        /// Whether or not to only expand to the basename of the directory.
        basename: bool,
    },
    /// The current date, using the given format.
    Date(PromptDateFormat),
    /// The dollar or pound character.
    DollarOrPound,
    /// Special marker indicating the end of a non-printing sequence of characters.
    EndNonPrintingSequence,
    /// The escape character.
    EscapeCharacter,
    /// An escaped sequence not otherwise recognized.
    EscapedSequence(String),
    /// The hostname of the system.
    Hostname {
        /// Whether or not to include only up to the first dot of the name.
        only_up_to_first_dot: bool,
    },
    /// A literal string.
    Literal(String),
    /// A newline character.
    Newline,
    /// The number of actively managed jobs.
    NumberOfManagedJobs,
    /// The base name of the shell.
    ShellBaseName,
    /// The release of the shell.
    ShellRelease,
    /// The version of the shell.
    ShellVersion,
    /// Special marker indicating the start of a non-printing sequence of characters.
    StartNonPrintingSequence,
    /// The base name of the terminal device.
    TerminalDeviceBaseName,
    /// The current time, using the given format.
    Time(PromptTimeFormat),
}

/// Format for a date in a prompt.
#[derive(Clone, Debug)]
#[cfg_attr(
    any(test, feature = "serde"),
    derive(PartialEq, Eq, serde::Serialize, serde::Deserialize)
)]
pub enum PromptDateFormat {
    /// A format including weekday, month, and date.
    WeekdayMonthDate,
    /// A customer string format.
    Custom(String),
}

/// Format for a time in a prompt.
#[derive(Clone, Debug)]
#[cfg_attr(
    any(test, feature = "serde"),
    derive(PartialEq, Eq, serde::Serialize, serde::Deserialize)
)]
pub enum PromptTimeFormat {
    /// A twelve-hour time format with AM/PM.
    TwelveHourAM,
    /// A twelve-hour time format (HHMMSS).
    TwelveHourHHMMSS,
    /// A twenty-four-hour time format (HHMM).
    TwentyFourHourHHMM,
    /// A twenty-four-hour time format (HHMMSS).
    TwentyFourHourHHMMSS,
}

// ============================================================================
// PEG-based implementation
// ============================================================================

peg::parser! {
    grammar peg_prompt_parser() for str {
        pub(crate) rule prompt() -> Vec<PromptPiece> =
            pieces:prompt_piece()*

        rule prompt_piece() -> PromptPiece =
            special_sequence() /
            literal_sequence()

        //
        // Reference: https://www.gnu.org/software/bash/manual/bash.html#Controlling-the-Prompt
        //
        rule special_sequence() -> PromptPiece =
            "\\a" { PromptPiece::BellCharacter } /
            "\\A" { PromptPiece::Time(PromptTimeFormat::TwentyFourHourHHMM) } /
            "\\d" { PromptPiece::Date(PromptDateFormat::WeekdayMonthDate) } /
            "\\D{" f:date_format() "}" { PromptPiece::Date(PromptDateFormat::Custom(f)) } /
            "\\e" { PromptPiece::EscapeCharacter } /
            "\\h" { PromptPiece::Hostname { only_up_to_first_dot: true } } /
            "\\H" { PromptPiece::Hostname { only_up_to_first_dot: false } } /
            "\\j" { PromptPiece::NumberOfManagedJobs } /
            "\\l" { PromptPiece::TerminalDeviceBaseName } /
            "\\n" { PromptPiece::Newline } /
            "\\r" { PromptPiece::CarriageReturn } /
            "\\s" { PromptPiece::ShellBaseName } /
            "\\t" { PromptPiece::Time(PromptTimeFormat::TwentyFourHourHHMMSS ) } /
            "\\T" { PromptPiece::Time(PromptTimeFormat::TwelveHourHHMMSS ) } /
            "\\@" { PromptPiece::Time(PromptTimeFormat::TwelveHourAM ) } /
            "\\u" { PromptPiece::CurrentUser } /
            "\\v" { PromptPiece::ShellVersion } /
            "\\V" { PromptPiece::ShellRelease } /
            "\\w" { PromptPiece::CurrentWorkingDirectory { tilde_replaced: true, basename: false, } } /
            "\\W" { PromptPiece::CurrentWorkingDirectory { tilde_replaced: true, basename: true, } } /
            "\\!" { PromptPiece::CurrentHistoryNumber } /
            "\\#" { PromptPiece::CurrentCommandNumber } /
            "\\$" { PromptPiece::DollarOrPound } /
            "\\" n:octal_number() { PromptPiece::AsciiCharacter(n) } /
            "\\\\" { PromptPiece::Backslash } /
            "\\[" { PromptPiece::StartNonPrintingSequence } /
            "\\]" { PromptPiece::EndNonPrintingSequence } /
            s:$("\\" [_]) { PromptPiece::EscapedSequence(s.to_owned()) }

        rule literal_sequence() -> PromptPiece =
            s:$((!special_sequence() [c])+) { PromptPiece::Literal(s.to_owned()) }

        rule date_format() -> String =
            s:$([c if c != '}']*) { s.to_owned() }

        rule octal_number() -> u32 =
            s:$(['0'..='7']*<1,3>) {? u32::from_str_radix(s, 8).or(Err("invalid octal number")) }
    }
}

fn peg_parse(s: &str) -> Result<Vec<PromptPiece>, error::WordParseError> {
    peg_prompt_parser::prompt(s).map_err(|e| error::WordParseError::Prompt(e.to_string()))
}

// ============================================================================
// Winnow-based implementation
// ============================================================================

#[cfg(feature = "winnow-parser")]
mod winnow_impl {
    use super::{PromptDateFormat, PromptPiece, PromptTimeFormat};
    use winnow::combinator::{alt, cut_err, delimited, empty, fail, repeat};
    use winnow::dispatch;
    use winnow::prelude::*;
    use winnow::token::{any, take_while};

    pub(super) fn parse(i: &mut &str) -> ModalResult<Vec<PromptPiece>> {
        repeat(0.., prompt_piece).parse_next(i)
    }

    fn prompt_piece(i: &mut &str) -> ModalResult<PromptPiece> {
        alt((special_sequence, literal_sequence)).parse_next(i)
    }

    fn special_sequence(i: &mut &str) -> ModalResult<PromptPiece> {
        '\\'.parse_next(i)?;
        alt((
            dispatch! { any;
                'a' => empty.value(PromptPiece::BellCharacter),
                'A' => empty.value(PromptPiece::Time(PromptTimeFormat::TwentyFourHourHHMM)),
                'd' => empty.value(PromptPiece::Date(PromptDateFormat::WeekdayMonthDate)),
                'D' => custom_date_format_tail,
                'e' => empty.value(PromptPiece::EscapeCharacter),
                'h' => empty.value(PromptPiece::Hostname { only_up_to_first_dot: true }),
                'H' => empty.value(PromptPiece::Hostname { only_up_to_first_dot: false }),
                'j' => empty.value(PromptPiece::NumberOfManagedJobs),
                'l' => empty.value(PromptPiece::TerminalDeviceBaseName),
                'n' => empty.value(PromptPiece::Newline),
                'r' => empty.value(PromptPiece::CarriageReturn),
                's' => empty.value(PromptPiece::ShellBaseName),
                't' => empty.value(PromptPiece::Time(PromptTimeFormat::TwentyFourHourHHMMSS)),
                'T' => empty.value(PromptPiece::Time(PromptTimeFormat::TwelveHourHHMMSS)),
                '@' => empty.value(PromptPiece::Time(PromptTimeFormat::TwelveHourAM)),
                'u' => empty.value(PromptPiece::CurrentUser),
                'v' => empty.value(PromptPiece::ShellVersion),
                'V' => empty.value(PromptPiece::ShellRelease),
                'w' => empty.value(PromptPiece::CurrentWorkingDirectory { tilde_replaced: true, basename: false }),
                'W' => empty.value(PromptPiece::CurrentWorkingDirectory { tilde_replaced: true, basename: true }),
                '!' => empty.value(PromptPiece::CurrentHistoryNumber),
                '#' => empty.value(PromptPiece::CurrentCommandNumber),
                '$' => empty.value(PromptPiece::DollarOrPound),
                '\\' => empty.value(PromptPiece::Backslash),
                '[' => empty.value(PromptPiece::StartNonPrintingSequence),
                ']' => empty.value(PromptPiece::EndNonPrintingSequence),
                _ => fail::<_, PromptPiece, _>,
            },
            // Octal: \nnn (1-3 octal digits) — these chars are not in the dispatch table above
            octal_number.map(PromptPiece::AsciiCharacter),
            // Any other escaped char: \x → EscapedSequence("\\x")
            any.map(|c: char| PromptPiece::EscapedSequence(format!("\\{c}"))),
        ))
        .parse_next(i)
    }

    fn custom_date_format_tail(i: &mut &str) -> ModalResult<PromptPiece> {
        let f = delimited('{', date_format, cut_err('}')).parse_next(i)?;
        Ok(PromptPiece::Date(PromptDateFormat::Custom(f)))
    }

    fn date_format(i: &mut &str) -> ModalResult<String> {
        take_while(0.., |c: char| c != '}')
            .map(str::to_owned)
            .parse_next(i)
    }

    fn octal_number(i: &mut &str) -> ModalResult<u32> {
        let digits = take_while(1..=3, |c: char| matches!(c, '0'..='7')).parse_next(i)?;
        // 1-3 octal digits always parse successfully (max 0o777 = 511 < u32::MAX)
        Ok(u32::from_str_radix(digits, 8).unwrap_or(0))
    }

    fn literal_sequence(i: &mut &str) -> ModalResult<PromptPiece> {
        take_while(1.., |c: char| c != '\\')
            .map(|s: &str| PromptPiece::Literal(s.to_owned()))
            .parse_next(i)
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Parses a shell prompt string using the default parser implementation.
///
/// # Arguments
///
/// * `s` - The prompt string to parse.
pub fn parse(s: &str) -> Result<Vec<PromptPiece>, error::WordParseError> {
    parse_with(s, ParserImpl::default())
}

/// Parses a shell prompt string using the specified parser implementation.
///
/// # Arguments
///
/// * `s` - The prompt string to parse.
/// * `impl_` - The parser implementation to use.
pub fn parse_with(s: &str, impl_: ParserImpl) -> Result<Vec<PromptPiece>, error::WordParseError> {
    match impl_ {
        ParserImpl::Peg => peg_parse(s),
        #[cfg(feature = "winnow-parser")]
        ParserImpl::Winnow => {
            use winnow::Parser as _;
            winnow_impl::parse
                .parse(s)
                .map_err(|e| error::WordParseError::Prompt(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use pretty_assertions::assert_eq;

    #[test]
    fn basic_prompt() -> Result<()> {
        assert_eq!(
            parse(r"\u@\h:\w$ ")?,
            &[
                PromptPiece::CurrentUser,
                PromptPiece::Literal("@".to_owned()),
                PromptPiece::Hostname {
                    only_up_to_first_dot: true
                },
                PromptPiece::Literal(":".to_owned()),
                PromptPiece::CurrentWorkingDirectory {
                    tilde_replaced: true,
                    basename: false
                },
                PromptPiece::Literal("$ ".to_owned()),
            ]
        );

        Ok(())
    }

    #[test]
    fn brackets_and_vars() -> Result<()> {
        assert_eq!(
            parse(r"\[$foo\]\u > ")?,
            &[
                PromptPiece::StartNonPrintingSequence,
                PromptPiece::Literal("$foo".to_owned()),
                PromptPiece::EndNonPrintingSequence,
                PromptPiece::CurrentUser,
                PromptPiece::Literal(" > ".to_owned()),
            ]
        );

        Ok(())
    }
}
