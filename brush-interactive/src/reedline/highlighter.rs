use std::str::Chars;

use super::refs;
use nu_ansi_term::{Color, Style};

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

pub(crate) struct ReedlineHighlighter {
    pub shell: refs::ShellRef,
}

impl reedline::Highlighter for ReedlineHighlighter {
    #[expect(clippy::significant_drop_tightening)]
    fn highlight(&self, line: &str, cursor: usize) -> reedline::StyledText {
        let shell = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.shell.lock())
        });

        let mut styled_input = StyledInputLine::new(shell.as_ref(), line, cursor);

        styled_input.style_and_append_program(line, 0);

        styled_input.styled
    }
}

enum CommandType {
    Function,
    Keyword,
    Builtin,
    Alias,
    External,
    NotFound,
    Unknown,
}

struct StyledInputLine<'a> {
    shell: &'a brush_core::Shell,
    cursor: usize,
    styled: reedline::StyledText,
    remaining_chars: Chars<'a>,
    current_char_index: usize,
    next_missing_style: Option<Style>,
}

impl<'a> StyledInputLine<'a> {
    fn new(shell: &'a brush_core::Shell, input_line: &'a str, cursor: usize) -> Self {
        Self {
            shell,
            cursor,
            styled: reedline::StyledText::new(),
            remaining_chars: input_line.chars(),
            current_char_index: 0,
            next_missing_style: None,
        }
    }

    fn style_and_append_program(&mut self, line: &str, global_offset: usize) {
        if let Ok(tokens) = brush_parser::tokenize_str_with_options(
            line,
            &(self.shell.parser_options().tokenizer_options()),
        ) {
            let mut saw_command_token = false;
            for token in tokens {
                match token {
                    brush_parser::Token::Operator(_op, token_location) => {
                        self.append_style(
                            styles::operator(),
                            global_offset + token_location.start.index,
                            global_offset + token_location.end.index,
                        );
                    }
                    brush_parser::Token::Word(w, token_location) => {
                        if let Ok(word_pieces) =
                            brush_parser::word::parse(w.as_str(), &self.shell.parser_options())
                        {
                            let default_text_style = self.get_style_for_word(
                                w.as_str(),
                                &token_location,
                                &mut saw_command_token,
                            );

                            for word_piece in word_pieces {
                                self.style_and_append_word_piece(
                                    word_piece,
                                    default_text_style,
                                    global_offset + token_location.start.index,
                                );
                            }
                        }
                    }
                }
            }

            self.skip_ahead(global_offset + line.len());
        } else {
            self.append_style(styles::default(), global_offset, global_offset + line.len());
        }
    }

    fn style_and_append_word_piece(
        &mut self,
        word_piece: brush_parser::word::WordPieceWithSource,
        default_text_style: Style,
        global_offset: usize,
    ) {
        self.skip_ahead(global_offset + word_piece.start_index);

        match word_piece.piece {
            brush_parser::word::WordPiece::SingleQuotedText(_)
            | brush_parser::word::WordPiece::AnsiCQuotedText(_)
            | brush_parser::word::WordPiece::EscapeSequence(_) => {
                self.append_style(
                    styles::quoted(),
                    global_offset + word_piece.start_index,
                    global_offset + word_piece.end_index,
                );
            }
            brush_parser::word::WordPiece::DoubleQuotedSequence(subpieces)
            | brush_parser::word::WordPiece::GettextDoubleQuotedSequence(subpieces) => {
                self.set_next_missing_style(styles::quoted());
                for subpiece in subpieces {
                    self.style_and_append_word_piece(subpiece, styles::quoted(), global_offset);
                }
                self.set_next_missing_style(styles::quoted());
            }
            brush_parser::word::WordPiece::ParameterExpansion(_)
            | brush_parser::word::WordPiece::TildePrefix(_) => {
                self.append_style(
                    styles::parameter(),
                    global_offset + word_piece.start_index,
                    global_offset + word_piece.end_index,
                );
            }
            brush_parser::word::WordPiece::BackquotedCommandSubstitution(command) => {
                self.set_next_missing_style(styles::command_substitution());
                self.style_and_append_program(
                    command.as_str(),
                    global_offset + word_piece.start_index + 1, /* account for opening backtick */
                );
                self.set_next_missing_style(styles::command_substitution());
            }
            brush_parser::word::WordPiece::CommandSubstitution(command) => {
                self.set_next_missing_style(styles::command_substitution());
                self.style_and_append_program(
                    command.as_str(),
                    global_offset + word_piece.start_index + 2, /* account for opening $( */
                );
                self.set_next_missing_style(styles::command_substitution());
            }
            brush_parser::word::WordPiece::ArithmeticExpression(_) => {
                // TODO: Consider individually highlighting pieces of the expression itself.
                self.append_style(
                    styles::arithmetic(),
                    global_offset + word_piece.start_index,
                    global_offset + word_piece.end_index,
                );
            }
            brush_parser::word::WordPiece::Text(_text) => {
                self.append_style(
                    default_text_style,
                    global_offset + word_piece.start_index,
                    global_offset + word_piece.end_index,
                );
            }
        }

        self.skip_ahead(global_offset + word_piece.end_index);
    }

    fn append_style(&mut self, style: Style, start: usize, end: usize) {
        // See if we need to cover a gap between this substring and the one that preceded it.
        if start > self.current_char_index {
            let missing_style = self.next_missing_style.unwrap_or_else(styles::comment);
            let missing_text: String = (&mut self.remaining_chars)
                .take(start - self.current_char_index)
                .collect();
            self.styled.push((missing_style, missing_text));
            self.current_char_index = start;
        }

        if end > start {
            let text: String = (&mut self.remaining_chars).take(end - start).collect();
            self.styled.push((style, text));
        }

        self.current_char_index = end;
    }

    fn skip_ahead(&mut self, dest: usize) {
        // Append a no-op style to make sure we cover any trailing gaps in the input line not
        // otherwise styled.
        self.append_style(Style::new(), dest, dest);
    }

    const fn set_next_missing_style(&mut self, style: Style) {
        self.next_missing_style = Some(style);
    }

    fn get_style_for_word(
        &self,
        w: &str,
        token_location: &brush_parser::TokenLocation,
        saw_command_token: &mut bool,
    ) -> Style {
        if !*saw_command_token {
            if w.contains('=') {
                styles::assignment()
            } else {
                *saw_command_token = true;
                match self.classify_possible_command(w, token_location) {
                    CommandType::Function => styles::function(),
                    CommandType::Keyword => styles::keyword(),
                    CommandType::Builtin => styles::builtin(),
                    CommandType::Alias => styles::alias(),
                    CommandType::External => styles::external_command(),
                    CommandType::NotFound => styles::not_found_command(),
                    CommandType::Unknown => styles::unknown_command(),
                }
            }
        } else {
            if self.shell.is_keyword(w) {
                styles::keyword()
            } else if w.starts_with('-') {
                styles::hyphen_option()
            } else {
                styles::default()
            }
        }
    }

    fn classify_possible_command(
        &self,
        name: &str,
        token_location: &brush_parser::TokenLocation,
    ) -> CommandType {
        if self.shell.is_keyword(name) {
            return CommandType::Keyword;
        } else if self.shell.aliases.contains_key(name) {
            return CommandType::Alias;
        } else if self.shell.funcs().get(name).is_some() {
            return CommandType::Function;
        } else if self.shell.builtins().contains_key(name) {
            return CommandType::Builtin;
        }

        // Short-circuit if the cursor is still in this token.
        if (self.cursor >= token_location.start.index) && (self.cursor <= token_location.end.index)
        {
            return CommandType::Unknown;
        }

        if name.contains(std::path::MAIN_SEPARATOR) {
            // TODO: Should check for executable-ness.
            let candidate_path = self.shell.absolute_path(std::path::Path::new(name));
            if candidate_path.exists() {
                CommandType::External
            } else {
                CommandType::NotFound
            }
        } else {
            if self.shell.find_first_executable_in_path(name).is_some() {
                CommandType::External
            } else {
                CommandType::NotFound
            }
        }
    }
}
