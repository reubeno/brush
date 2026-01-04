use nu_ansi_term::{Color, Style};

use crate::{highlighting, refs};

mod styles {
    use super::{Color, Style};

    pub fn default() -> Style {
        Style::new().fg(Color::White)
    }

    pub fn comment() -> Style {
        Style::new().fg(Color::DarkGray)
    }

    pub fn arithmetic() -> Style {
        Style::new().fg(Color::LightBlue)
    }

    pub fn parameter() -> Style {
        Style::new().fg(Color::LightMagenta)
    }

    pub fn command_substitution() -> Style {
        Style::new().fg(Color::LightBlue)
    }

    pub fn quoted() -> Style {
        Style::new().fg(Color::Yellow)
    }

    pub fn operator() -> Style {
        Style::new().fg(Color::White).italic()
    }

    pub fn assignment() -> Style {
        Style::new().fg(Color::LightGray).dimmed()
    }

    pub fn hyphen_option() -> Style {
        Style::new().fg(Color::White).italic()
    }

    pub fn function() -> Style {
        Style::new().bold().fg(Color::Yellow)
    }

    pub fn keyword() -> Style {
        Style::new().bold().fg(Color::LightYellow).italic()
    }

    pub fn builtin() -> Style {
        Style::new().bold().fg(Color::Green)
    }

    pub fn alias() -> Style {
        Style::new().bold().fg(Color::Cyan)
    }

    pub fn external_command() -> Style {
        Style::new().bold().fg(Color::Green)
    }

    pub fn not_found_command() -> Style {
        Style::new().bold().fg(Color::Red)
    }

    pub fn unknown_command() -> Style {
        Style::new().bold().fg(Color::White)
    }
}

pub(crate) struct ReedlineHighlighter<S: brush_core::ShellRuntime> {
    pub shell: refs::ShellRef<S>,
}

impl<S: brush_core::ShellRuntime> reedline::Highlighter for ReedlineHighlighter<S> {
    #[expect(clippy::significant_drop_tightening)]
    fn highlight(&self, line: &str, cursor: usize) -> reedline::StyledText {
        let shell = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.shell.lock())
        });

        let spans = highlighting::highlight_command(shell.as_ref(), line, cursor);

        let mut styled = reedline::StyledText::new();
        for span in spans {
            let style = kind_to_style(span.kind);
            styled.push((style, span.text(line).to_owned()));
        }

        styled
    }
}

fn kind_to_style(kind: highlighting::HighlightKind) -> Style {
    match kind {
        highlighting::HighlightKind::Default => styles::default(),
        highlighting::HighlightKind::Comment => styles::comment(),
        highlighting::HighlightKind::Arithmetic => styles::arithmetic(),
        highlighting::HighlightKind::Parameter => styles::parameter(),
        highlighting::HighlightKind::CommandSubstitution => styles::command_substitution(),
        highlighting::HighlightKind::Quoted => styles::quoted(),
        highlighting::HighlightKind::Operator => styles::operator(),
        highlighting::HighlightKind::Assignment => styles::assignment(),
        highlighting::HighlightKind::HyphenOption => styles::hyphen_option(),
        highlighting::HighlightKind::Function => styles::function(),
        highlighting::HighlightKind::Keyword => styles::keyword(),
        highlighting::HighlightKind::Builtin => styles::builtin(),
        highlighting::HighlightKind::Alias => styles::alias(),
        highlighting::HighlightKind::ExternalCommand => styles::external_command(),
        highlighting::HighlightKind::NotFoundCommand => styles::not_found_command(),
        highlighting::HighlightKind::UnknownCommand => styles::unknown_command(),
    }
}
