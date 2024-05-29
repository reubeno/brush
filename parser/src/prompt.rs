use crate::error;

#[derive(Clone)]
pub enum PromptPiece {
    AsciiCharacter(u32),
    Backslash,
    BellCharacter,
    CarriageReturn,
    CurrentCommandNumber,
    CurrentHistoryNumber,
    CurrentUser,
    CurrentWorkingDirectory {
        tilde_replaced: bool,
        basename: bool,
    },
    Date(PromptDateFormat),
    DollarOrPound,
    EndNonPrintingSequence,
    EscapeCharacter,
    Hostname {
        only_up_to_first_dot: bool,
    },
    Literal(String),
    Newline,
    NumberOfManagedJobs,
    ShellBaseName,
    ShellRelease,
    ShellVersion,
    StartNonPrintingSequence,
    TerminalDeviceBaseName,
    Time(PromptTimeFormat),
}

#[derive(Clone)]
pub enum PromptDateFormat {
    WeekdayMonthDate,
    Custom(String),
}

#[derive(Clone)]
pub enum PromptTimeFormat {
    TwelveHourAM,
    TwelveHourHHMMSS,
    TwentyFourHourHHMMSS,
}

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
            s:$(!"}" [c]+) { s.to_owned() }

        rule octal_number() -> u32 =
            s:$(['0'..='9']*<3,3>) {? u32::from_str_radix(s, 8).or(Err("invalid octal number")) }
    }
}

pub fn parse_prompt(s: &str) -> Result<Vec<PromptPiece>, error::WordParseError> {
    let result = prompt_parser::prompt(s).map_err(error::WordParseError::Prompt)?;
    Ok(result)
}
