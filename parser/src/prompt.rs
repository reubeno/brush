use anyhow::Result;

#[derive(Debug)]
pub enum ShellPromptPiece {
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
    Date(ShellPromptDateFormat),
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
    Time(ShellPromptTimeFormat),
}

#[derive(Debug)]
pub enum ShellPromptDateFormat {
    WeekdayMonthDate,
    Custom(String),
}

#[derive(Debug)]
pub enum ShellPromptTimeFormat {
    TwelveHourAM,
    TwelveHourHHMMSS,
    TwentyFourHourHHMMSS,
}

peg::parser! {
    grammar prompt_parser() for str {
        pub(crate) rule prompt() -> Vec<ShellPromptPiece> =
            pieces:prompt_piece()*

        rule prompt_piece() -> ShellPromptPiece =
            special_sequence() /
            literal_sequence()

        rule special_sequence() -> ShellPromptPiece =
            "\\a" { ShellPromptPiece::BellCharacter } /
            "\\d" { ShellPromptPiece::Date(ShellPromptDateFormat::WeekdayMonthDate) } /
            "\\D{" f:date_format() "}" { ShellPromptPiece::Date(ShellPromptDateFormat::Custom(f)) } /
            "\\e" { ShellPromptPiece::EscapeCharacter } /
            "\\h" { ShellPromptPiece::Hostname { only_up_to_first_dot: true } } /
            "\\H" { ShellPromptPiece::Hostname { only_up_to_first_dot: false } } /
            "\\j" { ShellPromptPiece::NumberOfManagedJobs } /
            "\\l" { ShellPromptPiece::TerminalDeviceBaseName } /
            "\\n" { ShellPromptPiece::Newline } /
            "\\r" { ShellPromptPiece::CarriageReturn } /
            "\\s" { ShellPromptPiece::ShellBaseName } /
            "\\t" { ShellPromptPiece::Time(ShellPromptTimeFormat::TwentyFourHourHHMMSS ) } /
            "\\T" { ShellPromptPiece::Time(ShellPromptTimeFormat::TwelveHourHHMMSS ) } /
            "\\@" { ShellPromptPiece::Time(ShellPromptTimeFormat::TwelveHourAM ) } /
            "\\u" { ShellPromptPiece::CurrentUser } /
            "\\v" { ShellPromptPiece::ShellVersion } /
            "\\V" { ShellPromptPiece::ShellRelease } /
            "\\w" { ShellPromptPiece::CurrentWorkingDirectory { tilde_replaced: true, basename: false, } } /
            "\\W" { ShellPromptPiece::CurrentWorkingDirectory { tilde_replaced: true, basename: true, } } /
            "\\!" { ShellPromptPiece::CurrentHistoryNumber } /
            "\\#" { ShellPromptPiece::CurrentCommandNumber } /
            "\\$" { ShellPromptPiece::DollarOrPound } /
            "\\" n:octal_number() { ShellPromptPiece::AsciiCharacter(n) } /
            "\\\\" { ShellPromptPiece::Backslash } /
            "\\[" { ShellPromptPiece::StartNonPrintingSequence } /
            "\\]" { ShellPromptPiece::EndNonPrintingSequence }

        rule literal_sequence() -> ShellPromptPiece =
            s:$((!special_sequence() [c])+) { ShellPromptPiece::Literal(s.to_owned()) }

        rule date_format() -> String =
            s:$(!"}" [c]+) { s.to_owned() }

        rule octal_number() -> u32 =
            s:$(['0'..='9']*<3,3>) {? u32::from_str_radix(s, 8).or(Err("invalid octal number")) }
    }
}

pub fn parse_prompt(s: &str) -> Result<Vec<ShellPromptPiece>> {
    Ok(prompt_parser::prompt(s)?)
}
