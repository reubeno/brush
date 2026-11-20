use crate::styles::kind_to_style;
use crate::{highlighting, refs};
use nu_ansi_term::Style;

pub(crate) struct ReedlineHighlighter<SE: brush_core::ShellExtensions> {
    pub shell: refs::ShellRef<SE>,
}

pub(crate) struct PlainTextHighlighter;

impl reedline::Highlighter for PlainTextHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> reedline::StyledText {
        let mut styled = reedline::StyledText::new();
        styled.push((Style::new(), line.to_owned()));
        styled
    }
}

impl<SE: brush_core::ShellExtensions> reedline::Highlighter for ReedlineHighlighter<SE> {
    #[expect(clippy::significant_drop_tightening)]
    fn highlight(&self, line: &str, cursor: usize) -> reedline::StyledText {
        let shell = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.shell.lock())
        });

        let highlighted = highlighting::highlight_command(shell.as_ref(), line, cursor);

        let mut styled = reedline::StyledText::new();
        for (kind, text) in highlighted.iter() {
            styled.push((kind_to_style(kind), text.to_owned()));
        }

        styled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reedline::Highlighter;

    #[test]
    fn plain_text_highlighter_uses_terminal_default_colors() {
        let styled = PlainTextHighlighter.highlight("echo hello", 0);

        assert_eq!(styled.buffer.len(), 1);
        assert_eq!(styled.buffer[0].0.foreground, None);
        assert_eq!(styled.buffer[0].0.background, None);
        assert_eq!(styled.buffer[0].1, "echo hello");
    }
}
