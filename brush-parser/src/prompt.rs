//! Parser for shell prompt syntax (e.g., `PS1`).

/// A piece of a prompt string.
#[derive(Clone)]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
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
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum PromptDateFormat {
    /// A format including weekday, month, and date.
    WeekdayMonthDate,
    /// A customer string format.
    Custom(String),
}

/// Format for a time in a prompt.
#[derive(Clone)]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
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
mod peg {
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

/// chumsky parser for prompt lines
pub mod chumsky {
    use chumsky::{
        IterParser, Parser,
        error::EmptyErr,
        prelude::{any, choice, just},
        text,
    };

    use super::{PromptDateFormat, PromptPiece, PromptTimeFormat};
    use crate::error;

    fn date<'a>() -> impl Parser<'a, &'a str, PromptPiece> {
        any()
            .filter(|c: &char| c.ne(&'}'))
            .repeated()
            .at_least(1)
            .collect::<String>()
            .delimited_by(just('{'), just('}'))
            .map(|d| PromptPiece::Date(PromptDateFormat::Custom(d)))
    }

    fn octal_number<'a>() -> impl Parser<'a, &'a str, PromptPiece> {
        text::digits(8)
            .at_least(1)
            .at_most(3)
            .to_slice()
            .try_map(|s: &str, _| {
                u32::from_str_radix(s, 8)
                    .map(PromptPiece::AsciiCharacter)
                    .map_err(|_| EmptyErr::default())
            })
    }

    fn escaped_char<'a>() -> impl Parser<'a, &'a str, PromptPiece> {
        let a = choice((
            just('a').to(PromptPiece::BellCharacter),
            just('A').to(PromptPiece::Time(PromptTimeFormat::TwentyFourHourHHMM)),
            just('d').to(PromptPiece::Date(PromptDateFormat::WeekdayMonthDate)),
            just('e').to(PromptPiece::EscapeCharacter),
            just('h').to(PromptPiece::Hostname {
                only_up_to_first_dot: true,
            }),
            just('H').to(PromptPiece::Hostname {
                only_up_to_first_dot: false,
            }),
            just('j').to(PromptPiece::NumberOfManagedJobs),
            just('l').to(PromptPiece::TerminalDeviceBaseName),
            just('n').to(PromptPiece::Newline),
            just('r').to(PromptPiece::CarriageReturn),
            just('s').to(PromptPiece::ShellBaseName),
            just('t').to(PromptPiece::Time(PromptTimeFormat::TwentyFourHourHHMMSS)),
            just('T').to(PromptPiece::Time(PromptTimeFormat::TwelveHourHHMMSS)),
            just('@').to(PromptPiece::Time(PromptTimeFormat::TwelveHourAM)),
        ));

        let b = choice((
            just('u').to(PromptPiece::CurrentUser),
            just('v').to(PromptPiece::ShellVersion),
            just('V').to(PromptPiece::ShellRelease),
            just('w').to(PromptPiece::CurrentWorkingDirectory {
                tilde_replaced: true,
                basename: false,
            }),
            just('W').to(PromptPiece::CurrentWorkingDirectory {
                tilde_replaced: true,
                basename: true,
            }),
            just('!').to(PromptPiece::CurrentHistoryNumber),
            just('#').to(PromptPiece::CurrentCommandNumber),
            just('$').to(PromptPiece::DollarOrPound),
            just('\\').to(PromptPiece::Backslash),
            just('[').to(PromptPiece::StartNonPrintingSequence),
            just(']').to(PromptPiece::EndNonPrintingSequence),
            just('D').ignore_then(date()),
            octal_number(),
        ));

        choice((a, b))
    }

    fn escaped_special<'a>() -> impl Parser<'a, &'a str, PromptPiece> {
        just('\\').ignore_then(escaped_char())
    }

    fn literal_sequence<'a>() -> impl Parser<'a, &'a str, PromptPiece> {
        any()
            .filter(|c: &char| c.ne(&'{'))
            .repeated()
            .collect::<String>()
            .map(|l| PromptPiece::Literal(l))
    }

    fn prompt_piece<'a>() -> impl Parser<'a, &'a str, PromptPiece> {
        choice((escaped_special(), literal_sequence()))
    }

    fn prompt<'a>() -> impl Parser<'a, &'a str, Vec<PromptPiece>> {
        prompt_piece().repeated().collect()
    }
    /// Parses a shell prompt string.
    ///
    /// # Arguments
    ///
    /// * `s` - The prompt string to parse.
    pub fn parse(s: &str) -> Result<Vec<PromptPiece>, error::WordParseError> {
        let result = prompt()
            .parse(s)
            .into_result()
            .map_err(|e| error::WordParseError::Prompt(e.into()))?;

        Ok(result)
    }
}

pub use peg::parse;
