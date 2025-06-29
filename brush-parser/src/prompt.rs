//! Parser for shell prompt syntax (e.g., `PS1`).

/// A piece of a prompt string.
#[derive(Clone)]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize, Debug))]
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
#[derive(Clone)]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize, Debug))]
pub enum PromptDateFormat {
    /// A format including weekday, month, and date.
    WeekdayMonthDate,
    /// A customer string format.
    Custom(String),
}

/// Format for a time in a prompt.
#[derive(Clone)]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize, Debug))]
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

/// peg parser for prompt lines
pub mod peg {
    use super::{PromptDateFormat, PromptPiece, PromptTimeFormat};
    use crate::error;

    peg::parser! {
        grammar prompt_parser() for str {
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
                "\\]" { PromptPiece::EndNonPrintingSequence }

            rule literal_sequence() -> PromptPiece =
                s:$((!special_sequence() [c])+) { PromptPiece::Literal(s.to_owned()) }

            rule date_format() -> String =
                s:$([c if c != '}']*) { s.to_owned() }

            rule octal_number() -> u32 =
                s:$(['0'..='9']*<3,3>) {? u32::from_str_radix(s, 8).or(Err("invalid octal number")) }
        }
    }

    /// Parses a shell prompt string.
    ///
    /// # Arguments
    ///
    /// * `s` - The prompt string to parse.
    pub fn parse(s: &str) -> Result<Vec<PromptPiece>, error::WordParseError> {
        let result =
            prompt_parser::prompt(s).map_err(|e| error::WordParseError::Prompt(e.into()))?;
        Ok(result)
    }
}

/// winnow parser for prompt lines
pub mod winnow {
    use winnow::Result;
    use winnow::combinator::{alt, delimited, dispatch, empty, fail, preceded, repeat, trace};
    use winnow::prelude::*;
    use winnow::stream::AsChar;
    use winnow::token::{any, take_till, take_until, take_while};

    use crate::error;

    use super::{PromptDateFormat, PromptPiece, PromptTimeFormat};

    fn date(input: &mut &str) -> Result<PromptPiece> {
        let fmt =
            trace("custom_date", delimited('{', take_until(1.., '}'), '}')).parse_next(input)?;

        Ok(PromptPiece::Date(PromptDateFormat::Custom(fmt.to_owned())))
    }

    fn octal_number(input: &mut &str) -> Result<PromptPiece> {
        trace(
            "octal_number",
            take_while(1..=3, AsChar::is_oct_digit)
                .try_map(|s| u32::from_str_radix(s, 8).map(PromptPiece::AsciiCharacter)),
        )
        .parse_next(input)
    }

    fn literal_sequence_inner<'a>(input: &mut &'a str) -> Result<&'a str> {
        take_till(1.., |c| c == '\\').parse_next(input)
    }

    fn escaped_char(input: &mut &str) -> Result<PromptPiece> {
        trace("escaped",
        alt((dispatch! { any;
            'a' => empty.value(PromptPiece::BellCharacter),
            'A' => empty.value(PromptPiece::Time(PromptTimeFormat::TwentyFourHourHHMM)),
            'd' => empty.value(PromptPiece::Date(PromptDateFormat::WeekdayMonthDate)),
            'e' => empty.value(PromptPiece::EscapeCharacter),
            'h' => empty.value(PromptPiece::Hostname { only_up_to_first_dot: true }),
            'H' => empty.value(PromptPiece::Hostname { only_up_to_first_dot: false }),
            'j' => empty.value(PromptPiece::NumberOfManagedJobs),
            'l' => empty.value(PromptPiece::TerminalDeviceBaseName),
            'n' => empty.value(PromptPiece::Newline),
            'r' => empty.value(PromptPiece::CarriageReturn),
            's' => empty.value(PromptPiece::ShellBaseName),
            't' => empty.value(PromptPiece::Time(PromptTimeFormat::TwentyFourHourHHMMSS )),
            'T' => empty.value(PromptPiece::Time(PromptTimeFormat::TwelveHourHHMMSS )),
            '@' => empty.value(PromptPiece::Time(PromptTimeFormat::TwelveHourAM )),
            'u' => empty.value(PromptPiece::CurrentUser),
            'v' => empty.value(PromptPiece::ShellVersion),
            'V' => empty.value(PromptPiece::ShellRelease),
            'w' => empty.value(PromptPiece::CurrentWorkingDirectory { tilde_replaced: true, basename: false, }),
            'W' => empty.value(PromptPiece::CurrentWorkingDirectory { tilde_replaced: true, basename: true, }),
            '!' => empty.value(PromptPiece::CurrentHistoryNumber),
            '#' => empty.value(PromptPiece::CurrentCommandNumber),
            '$' => empty.value(PromptPiece::DollarOrPound),
            '\\' => empty.value(PromptPiece::Backslash),
            '[' => empty.value(PromptPiece::StartNonPrintingSequence),
            ']' => empty.value(PromptPiece::EndNonPrintingSequence),
            'D' => date,
            _ => fail::<_, PromptPiece, _>,
        }, octal_number, literal_sequence_inner.map(|s| {
            let mut lit = "\\".to_string();
            lit.push_str(s);
            PromptPiece::Literal(lit)
        })
        ))
        ).parse_next(input)
    }

    fn escaped_special(input: &mut &str) -> Result<PromptPiece> {
        preceded('\\', escaped_char).parse_next(input)
    }

    fn literal_sequence(input: &mut &str) -> Result<PromptPiece> {
        literal_sequence_inner
            .map(|lit| PromptPiece::Literal(lit.to_owned()))
            .parse_next(input)
    }

    fn prompt_piece(input: &mut &str) -> Result<PromptPiece> {
        trace("prompt_piece", alt((escaped_special, literal_sequence))).parse_next(input)
    }

    fn prompt(input: &mut &str) -> Result<Vec<PromptPiece>> {
        repeat(1.., prompt_piece).parse_next(input)
    }

    /// Parses a shell prompt string.
    ///
    /// # Arguments
    ///
    /// * `s` - The prompt string to parse.
    pub fn parse(s: &str) -> Result<Vec<PromptPiece>, error::WordParseError> {
        let result = prompt
            .parse(s)
            .map_err(|e| error::WordParseError::Prompt(e.into()))?;

        Ok(result)
    }
}

pub use winnow::parse;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn literal_newline() {
        let prompt_winnow = winnow::parse("\n").unwrap();
        let prompt_peg = peg::parse("\n").unwrap();

        assert_eq!(prompt_winnow, prompt_peg);
    }

    #[test]
    fn complex_ps1() {
        let ps1 = r#"\[[1;2;32m\]lu_zero\[[0m\] in \[[1;38;2;32;144;16m\]mneme\[[0m\] in \[[1;36m\]brush\[[0m\] on \[[1;35m\] \[[0m\]\[[1;35m\]winnow-parsers\[[0m\] \[[1;31m\][\[[0m\]\[[1;31m\]\$\[[0m\]\[[1;31m\]!\[[0m\]\[[1;31m\]?\[[0m\]\[[1;31m\]]\[[0m\] via \[[1;31m\]🦀 \[[0m\]\[[1;31m\]v1.88.0\[[0m\]\[[1;31m\] \[[0m\]took \[[1;33m\]3m37s\[[0m\] \[[1;32m\]❯\[[0m\]
            "#;
        let prompt_winnow = winnow::parse(&ps1).unwrap();
        let prompt_peg = peg::parse(&ps1).unwrap();

        assert_eq!(prompt_winnow, prompt_peg);
    }
}
