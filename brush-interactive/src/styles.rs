//! The default syntax highlighting palette shared by input backends.

use crate::highlighting;
use nu_ansi_term::{Color, Style};

fn default() -> Style {
    Style::new().fg(Color::Default)
}

fn comment() -> Style {
    Style::new().fg(Color::DarkGray)
}

fn arithmetic() -> Style {
    Style::new().fg(Color::LightBlue)
}

fn parameter() -> Style {
    Style::new().fg(Color::LightMagenta)
}

fn command_substitution() -> Style {
    Style::new().fg(Color::LightBlue)
}

fn quoted() -> Style {
    Style::new().fg(Color::Yellow)
}

fn operator() -> Style {
    Style::new().fg(Color::Default).italic()
}

fn assignment() -> Style {
    Style::new().fg(Color::LightGray).dimmed()
}

fn hyphen_option() -> Style {
    Style::new().fg(Color::Default).italic()
}

fn function() -> Style {
    Style::new().bold().fg(Color::Yellow)
}

fn keyword() -> Style {
    Style::new().bold().fg(Color::LightYellow).italic()
}

fn builtin() -> Style {
    Style::new().bold().fg(Color::Green)
}

fn alias() -> Style {
    Style::new().bold().fg(Color::Cyan)
}

fn external_command() -> Style {
    Style::new().bold().fg(Color::Green)
}

fn not_found_command() -> Style {
    Style::new().bold().fg(Color::Red)
}

fn unknown_command() -> Style {
    Style::new().bold().fg(Color::Default)
}

pub(crate) fn kind_to_style(kind: highlighting::HighlightKind) -> Style {
    match kind {
        highlighting::HighlightKind::Default => default(),
        highlighting::HighlightKind::Comment => comment(),
        highlighting::HighlightKind::Arithmetic => arithmetic(),
        highlighting::HighlightKind::Parameter => parameter(),
        highlighting::HighlightKind::CommandSubstitution => command_substitution(),
        highlighting::HighlightKind::Quoted => quoted(),
        highlighting::HighlightKind::Operator => operator(),
        highlighting::HighlightKind::Assignment => assignment(),
        highlighting::HighlightKind::HyphenOption => hyphen_option(),
        highlighting::HighlightKind::Function => function(),
        highlighting::HighlightKind::Keyword => keyword(),
        highlighting::HighlightKind::Builtin => builtin(),
        highlighting::HighlightKind::Alias => alias(),
        highlighting::HighlightKind::ExternalCommand => external_command(),
        highlighting::HighlightKind::NotFoundCommand => not_found_command(),
        highlighting::HighlightKind::UnknownCommand => unknown_command(),
    }
}
