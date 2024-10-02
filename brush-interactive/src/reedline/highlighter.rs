pub(crate) struct ReedlineHighlighter {}

impl reedline::Highlighter for ReedlineHighlighter {
    #[allow(clippy::cast_sign_loss)]
    fn highlight(&self, line: &str, _cursor: usize) -> reedline::StyledText {
        let mut styled_text = reedline::StyledText::new();

        if let Ok(tokens) = brush_parser::tokenize_str(line) {
            let mut last_pos = 0;
            for token in tokens {
                let style;
                let text;
                let loc;

                match token {
                    brush_parser::Token::Operator(o, token_location) => {
                        style = nu_ansi_term::Style::new().fg(nu_ansi_term::Color::Cyan);
                        text = o;
                        loc = token_location;
                    }
                    brush_parser::Token::Word(w, token_location) => {
                        style = nu_ansi_term::Style::new().fg(nu_ansi_term::Color::White);
                        text = w;
                        loc = token_location;
                    }
                }

                let start = loc.start.index as usize;

                if start > last_pos {
                    let missing_style =
                        nu_ansi_term::Style::new().fg(nu_ansi_term::Color::DarkGray);
                    let missing_text = &line[last_pos..start];
                    styled_text.push((missing_style, missing_text.to_owned()));
                }

                last_pos = loc.end.index as usize;

                styled_text.push((style, text));
            }

            if last_pos < line.len() {
                let missing_style = nu_ansi_term::Style::new().fg(nu_ansi_term::Color::DarkGray);
                let missing_text = &line[last_pos..];
                styled_text.push((missing_style, missing_text.to_owned()));
            }
        } else {
            let style = nu_ansi_term::Style::new().fg(nu_ansi_term::Color::White);
            styled_text.push((style, line.to_string()));
        }

        styled_text
    }
}
