//! Generic syntax highlighting for shell commands.
//!
//! This module provides semantic tagging of shell command strings without
//! imposing any specific styling. Consumers can map the semantic categories
//! to their own color schemes or styles.

/// Semantic category for a highlighted span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightKind {
    /// Default text
    Default,
    /// Comment text
    Comment,
    /// Arithmetic expression
    Arithmetic,
    /// Parameter expansion (variables, etc.)
    Parameter,
    /// Command substitution
    CommandSubstitution,
    /// Quoted text
    Quoted,
    /// Operator (|, &&, etc.)
    Operator,
    /// Variable assignment
    Assignment,
    /// Hyphen-prefixed option
    HyphenOption,
    /// Function definition
    Function,
    /// Shell keyword
    Keyword,
    /// Builtin command
    Builtin,
    /// Alias
    Alias,
    /// External command (found in PATH)
    ExternalCommand,
    /// Command not found
    NotFoundCommand,
    /// Unknown command (cursor still in token)
    UnknownCommand,
}

/// A highlighted span of text with semantic meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    /// Byte range of this span within the input string.
    pub range: std::ops::Range<usize>,
    /// Semantic category of this span.
    pub kind: HighlightKind,
}

impl HighlightSpan {
    /// Creates a new highlight span over the given byte range.
    #[must_use]
    pub const fn new(range: std::ops::Range<usize>, kind: HighlightKind) -> Self {
        Self { range, kind }
    }
}

/// The result of highlighting a line: the spans plus the line they index into.
///
/// Pairing the two means span text is resolved against the original input
/// rather than a separately-supplied (and possibly mismatched) string.
pub struct Highlighted<'a> {
    line: &'a str,
    spans: Vec<HighlightSpan>,
}

impl<'a> Highlighted<'a> {
    /// The line that was highlighted.
    #[must_use]
    pub const fn line(&self) -> &'a str {
        self.line
    }

    /// The spans, in order, covering the entire line.
    #[must_use]
    pub fn spans(&self) -> &[HighlightSpan] {
        &self.spans
    }

    /// Returns the text of `span` within the highlighted line.
    ///
    /// `span` is expected to be one of [`Self::spans`]; a range that is out of
    /// bounds or off a UTF-8 char boundary (debug-asserted) yields `""` rather
    /// than panicking.
    #[must_use]
    pub fn text(&self, span: &HighlightSpan) -> &'a str {
        debug_assert!(
            self.line.is_char_boundary(span.range.start),
            "highlight span start {} is not a UTF-8 char boundary in {:?}",
            span.range.start,
            self.line,
        );
        debug_assert!(
            self.line.is_char_boundary(span.range.end),
            "highlight span end {} is not a UTF-8 char boundary in {:?}",
            span.range.end,
            self.line,
        );
        self.line.get(span.range.clone()).unwrap_or("")
    }

    /// Iterates `(kind, text)` for each span, with text borrowed from the line.
    pub fn iter(&self) -> impl Iterator<Item = (HighlightKind, &'a str)> + '_ {
        self.spans.iter().map(|span| (span.kind, self.text(span)))
    }
}

/// Highlights a shell command string.
///
/// # Arguments
/// * `shell` - Reference to the shell for context (aliases, functions, builtins, etc.)
/// * `line` - The command string to highlight
/// * `cursor` - Current cursor position (byte offset)
///
/// # Returns
/// The highlighted line: spans covering the entire input, paired with the input.
#[must_use]
pub fn highlight_command<'a>(
    shell: &brush_core::Shell<impl brush_core::ShellExtensions>,
    line: &'a str,
    cursor: usize,
) -> Highlighted<'a> {
    let mut highlighter = Highlighter::new(shell, line, cursor);
    highlighter.highlight_program(line, 0);
    Highlighted {
        line,
        spans: highlighter.spans,
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

struct Highlighter<'a, SE: brush_core::ShellExtensions> {
    shell: &'a brush_core::Shell<SE>,
    /// The topmost input line; span offsets stored in `spans` index into this string.
    input_line: &'a str,
    cursor: usize,
    spans: Vec<HighlightSpan>,
    current_byte_index: usize,
    next_missing_kind: Option<HighlightKind>,
}

impl<'a, SE: brush_core::ShellExtensions> Highlighter<'a, SE> {
    const fn new(shell: &'a brush_core::Shell<SE>, input_line: &'a str, cursor: usize) -> Self {
        Self {
            shell,
            input_line,
            cursor,
            spans: Vec::new(),
            current_byte_index: 0,
            next_missing_kind: None,
        }
    }

    fn highlight_program(&mut self, line: &str, global_offset: usize) {
        if let Ok(tokens) = brush_parser::tokenize_str_with_options(
            line,
            &(self.shell.parser_options().tokenizer_options()),
        ) {
            let mut saw_command_token = false;

            // Tokenizer offsets are *character* indices into `line`; slicing needs bytes.
            // `char_indices()` gives the byte offset of each char, plus a sentinel for the
            // end, so a lookup is O(1) and the whole line costs one pass (not O(n²)).
            let char_byte_offsets: Vec<usize> = line
                .char_indices()
                .map(|(byte_idx, _)| byte_idx)
                .chain(std::iter::once(line.len()))
                .collect();
            let byte_offset = |char_offset: usize| {
                char_byte_offsets
                    .get(char_offset)
                    .copied()
                    .unwrap_or(line.len())
            };

            for token in tokens {
                match token {
                    brush_parser::Token::Operator(_op, token_location) => {
                        let start = global_offset + byte_offset(token_location.start.index);
                        let end = global_offset + byte_offset(token_location.end.index);
                        self.append_span(HighlightKind::Operator, start..end);
                    }
                    brush_parser::Token::Word(w, token_location) => {
                        let start_byte = byte_offset(token_location.start.index);
                        let end_byte = byte_offset(token_location.end.index);

                        // Parse the raw slice from `line`, not `w.as_str()`: the tokenizer may
                        // drop chars from `w` (e.g. `\<newline>` continuations), so offsets into
                        // `w` no longer map onto `line`; offsets into the raw slice do.
                        let raw_word_text = line.get(start_byte..end_byte).unwrap_or("");
                        if let Ok(word_pieces) =
                            brush_parser::word::parse(raw_word_text, &self.shell.parser_options())
                        {
                            let token_range =
                                (global_offset + start_byte)..(global_offset + end_byte);

                            // Classify against the tokenized form `w` (the logical word) so
                            // command lookups ignore mid-word line continuations.
                            let default_text_kind = self.get_kind_for_word(
                                w.as_str(),
                                &token_range,
                                &mut saw_command_token,
                            );

                            for word_piece in word_pieces {
                                self.highlight_word_piece(
                                    word_piece,
                                    default_text_kind,
                                    token_range.start,
                                );
                            }
                        }
                    }
                }
            }

            self.skip_ahead(global_offset + line.len());
        } else {
            self.append_span(
                HighlightKind::Default,
                global_offset..global_offset + line.len(),
            );
        }
    }

    fn highlight_word_piece(
        &mut self,
        word_piece: brush_parser::word::WordPieceWithSource,
        default_text_kind: HighlightKind,
        global_offset: usize,
    ) {
        let piece =
            (global_offset + word_piece.start_index)..(global_offset + word_piece.end_index);
        self.skip_ahead(piece.start);

        match word_piece.piece {
            brush_parser::word::WordPiece::SingleQuotedText(_)
            | brush_parser::word::WordPiece::AnsiCQuotedText(_)
            | brush_parser::word::WordPiece::EscapeSequence(_) => {
                self.append_span(HighlightKind::Quoted, piece.clone());
            }
            brush_parser::word::WordPiece::DoubleQuotedSequence(subpieces)
            | brush_parser::word::WordPiece::GettextDoubleQuotedSequence(subpieces) => {
                self.set_next_missing_kind(HighlightKind::Quoted);
                for subpiece in subpieces {
                    self.highlight_word_piece(subpiece, HighlightKind::Quoted, global_offset);
                }
                self.set_next_missing_kind(HighlightKind::Quoted);
            }
            brush_parser::word::WordPiece::ParameterExpansion(_)
            | brush_parser::word::WordPiece::TildeExpansion(_) => {
                self.append_span(HighlightKind::Parameter, piece.clone());
            }
            brush_parser::word::WordPiece::BackquotedCommandSubstitution(command) => {
                self.set_next_missing_kind(HighlightKind::CommandSubstitution);
                self.highlight_program(
                    command.as_str(),
                    piece.start + 1, /* opening backtick */
                );
                self.set_next_missing_kind(HighlightKind::CommandSubstitution);
            }
            brush_parser::word::WordPiece::CommandSubstitution(command) => {
                self.set_next_missing_kind(HighlightKind::CommandSubstitution);
                self.highlight_program(command.as_str(), piece.start + 2 /* opening $( */);
                self.set_next_missing_kind(HighlightKind::CommandSubstitution);
            }
            brush_parser::word::WordPiece::ArithmeticExpression(_) => {
                // TODO(highlighting): Consider individually highlighting pieces of the expression
                // itself.
                self.append_span(HighlightKind::Arithmetic, piece.clone());
            }
            brush_parser::word::WordPiece::Text(_text) => {
                self.append_span(default_text_kind, piece.clone());
            }
        }

        self.skip_ahead(piece.end);
    }

    fn append_span(&mut self, kind: HighlightKind, range: std::ops::Range<usize>) {
        debug_assert!(
            self.input_line.is_char_boundary(range.start),
            "span start {} is not a UTF-8 char boundary in {:?}",
            range.start,
            self.input_line,
        );
        debug_assert!(
            self.input_line.is_char_boundary(range.end),
            "span end {} is not a UTF-8 char boundary in {:?}",
            range.end,
            self.input_line,
        );

        // See if we need to cover a gap between this substring and the one that preceded it.
        if range.start > self.current_byte_index {
            let missing_kind = self.next_missing_kind.unwrap_or(HighlightKind::Comment);
            self.spans.push(HighlightSpan::new(
                self.current_byte_index..range.start,
                missing_kind,
            ));
            self.current_byte_index = range.start;
        }

        let end = range.end;
        if !range.is_empty() {
            self.spans.push(HighlightSpan::new(range, kind));
        }

        self.current_byte_index = end;
    }

    fn skip_ahead(&mut self, dest: usize) {
        // Append a no-op span to make sure we cover any trailing gaps in the input line not
        // otherwise styled.
        self.append_span(HighlightKind::Default, dest..dest);
    }

    const fn set_next_missing_kind(&mut self, kind: HighlightKind) {
        self.next_missing_kind = Some(kind);
    }

    fn get_kind_for_word(
        &self,
        w: &str,
        token_range: &std::ops::Range<usize>,
        saw_command_token: &mut bool,
    ) -> HighlightKind {
        if !*saw_command_token {
            if w.contains('=') {
                HighlightKind::Assignment
            } else {
                *saw_command_token = true;
                match self.classify_possible_command(w, token_range) {
                    CommandType::Function => HighlightKind::Function,
                    CommandType::Keyword => HighlightKind::Keyword,
                    CommandType::Builtin => HighlightKind::Builtin,
                    CommandType::Alias => HighlightKind::Alias,
                    CommandType::External => HighlightKind::ExternalCommand,
                    CommandType::NotFound => HighlightKind::NotFoundCommand,
                    CommandType::Unknown => HighlightKind::UnknownCommand,
                }
            }
        } else {
            if self.shell.is_keyword(w) {
                HighlightKind::Keyword
            } else if w.starts_with('-') {
                HighlightKind::HyphenOption
            } else {
                HighlightKind::Default
            }
        }
    }

    fn classify_possible_command(
        &self,
        name: &str,
        token_range: &std::ops::Range<usize>,
    ) -> CommandType {
        if self.shell.is_keyword(name) {
            return CommandType::Keyword;
        } else if self.shell.aliases().contains_key(name) {
            return CommandType::Alias;
        } else if self.shell.funcs().get(name).is_some() {
            return CommandType::Function;
        } else if self.shell.builtins().contains_key(name) {
            return CommandType::Builtin;
        }

        // Short-circuit if the cursor is still in this token (inclusive of its end, so a
        // command still being typed isn't prematurely flagged as not-found).
        if self.cursor >= token_range.start && self.cursor <= token_range.end {
            return CommandType::Unknown;
        }

        if brush_core::sys::fs::contains_path_separator(name) {
            // TODO(highlighting): Should check for executable-ness.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_highlight_simple_command() {
        let shell = brush_core::Shell::builder().build().await.unwrap();
        let line = "somecommand hello";
        // Use cursor position at the end so we get final highlighting
        let highlighted = highlight_command(&shell, line, line.len());

        // Should have at least 2 spans
        assert!(!highlighted.spans().is_empty());

        // Verify highlighting produces spans that cover the input
        let total_covered: usize = highlighted.spans().iter().map(|s| s.range.len()).sum();
        assert_eq!(total_covered, line.len(), "Spans should cover entire input");

        // The command should be classified as something (NotFound, External, etc.)
        let cmd_span = highlighted
            .spans()
            .iter()
            .find(|s| highlighted.text(s) == "somecommand");
        assert!(cmd_span.is_some(), "Should have a span for the command");
    }

    #[tokio::test]
    async fn test_highlight_quoted_string() {
        let shell = brush_core::Shell::builder().build().await.unwrap();
        let line = r#"echo "hello world""#;
        let highlighted = highlight_command(&shell, line, 0);

        // Should have spans for: echo, space, "hello world"
        assert!(!highlighted.spans().is_empty());

        // Check that quoted parts are marked as Quoted
        assert!(
            highlighted
                .spans()
                .iter()
                .any(|s| s.kind == HighlightKind::Quoted)
        );
    }

    #[tokio::test]
    async fn test_highlight_parameter_expansion() {
        let shell = brush_core::Shell::builder().build().await.unwrap();
        let line = "echo $HOME";
        let highlighted = highlight_command(&shell, line, 0);

        // Should have spans including a parameter expansion
        assert!(
            highlighted
                .spans()
                .iter()
                .any(|s| s.kind == HighlightKind::Parameter)
        );
    }

    #[tokio::test]
    async fn test_highlight_covers_entire_input() {
        let shell = brush_core::Shell::builder().build().await.unwrap();
        let line = "echo hello world";
        let highlighted = highlight_command(&shell, line, 0);

        // Verify that spans cover the entire input (no gaps)
        let mut covered = vec![false; line.len()];
        for span in highlighted.spans() {
            for item in covered
                .iter_mut()
                .take(span.range.end)
                .skip(span.range.start)
            {
                *item = true;
            }
        }

        assert!(covered.iter().all(|&c| c), "Not all characters are covered");
    }

    /// Asserts the invariants every highlighter output must satisfy (mirrors
    /// `fuzz/fuzz_targets/fuzz_highlight.rs`).
    fn assert_spans_are_valid(highlighted: &Highlighted<'_>) {
        let line = highlighted.line();

        // 1. Each span is in-range and lands on UTF-8 char boundaries.
        for span in highlighted.spans() {
            assert!(
                span.range.start <= span.range.end,
                "span has start > end: {span:?} (line={line:?})",
            );
            assert!(
                span.range.end <= line.len(),
                "span end exceeds line length: {span:?} (line.len()={})",
                line.len(),
            );
            assert!(
                line.is_char_boundary(span.range.start),
                "span start not on char boundary: {span:?} (line={line:?}, bytes={:?})",
                line.as_bytes(),
            );
            assert!(
                line.is_char_boundary(span.range.end),
                "span end not on char boundary: {span:?} (line={line:?}, bytes={:?})",
                line.as_bytes(),
            );
        }

        // 2. Spans are ordered and contiguous, covering the entire input.
        let mut next_expected_start = 0usize;
        for span in highlighted.spans() {
            assert_eq!(
                span.range.start, next_expected_start,
                "spans are not contiguous: {span:?} (expected start={next_expected_start}, line={line:?})",
            );
            next_expected_start = span.range.end;
        }
        assert_eq!(
            next_expected_start,
            line.len(),
            "spans do not cover entire input (covered {next_expected_start} of {}, line={line:?})",
            line.len(),
        );

        // 3. Resolving each span's text must not panic.
        for (_, _) in highlighted.iter() {}
    }

    #[tokio::test]
    async fn test_highlight_multibyte_chars_in_word_does_not_panic() {
        // Regression: a multibyte word followed by another token used to panic from
        // mixing char indices (tokenizer) with byte indices (word parser).
        let shell = brush_core::Shell::builder().build().await.unwrap();
        let line = ": 爸爸 /";
        let highlighted = highlight_command(&shell, line, line.len());

        assert_spans_are_valid(&highlighted);
    }

    #[tokio::test]
    async fn test_highlight_multibyte_chars_partial_input_does_not_panic() {
        // Simulate intermediate keystrokes while a user is typing the line.
        let shell = brush_core::Shell::builder().build().await.unwrap();
        let full = ": 爸爸 /";
        // Every char-boundary prefix must highlight without panicking.
        for (boundary, _) in full
            .char_indices()
            .chain(std::iter::once((full.len(), ' ')))
        {
            // `boundary` is sourced from char_indices(), so slicing is on a char boundary.
            #[allow(clippy::string_slice)]
            let line = &full[..boundary];
            let highlighted = highlight_command(&shell, line, line.len());
            assert_spans_are_valid(&highlighted);
        }
    }

    #[tokio::test]
    async fn test_highlight_multibyte_in_various_positions() {
        let shell = brush_core::Shell::builder().build().await.unwrap();
        let cases = [
            "爸",
            "爸 x",
            "x 爸",
            "echo 爸爸",
            "爸爸=value",
            "\"爸爸\" /",
            "$爸",
            "$(爸爸) /",
            "`爸爸` /",
            "# 爸爸 comment",
        ];
        for line in cases {
            let highlighted = highlight_command(&shell, line, line.len());
            assert_spans_are_valid(&highlighted);
        }
    }

    #[tokio::test]
    async fn test_highlight_issue_1128_multibyte_then_paren() {
        // Regression for #1128: a 2-byte char immediately followed by `(` panicked
        // because the operator's char index was sliced as a byte offset, landing
        // mid-character. Exercise the reported chars, including the keystroke-by-
        // keystroke sequence (the char alone, then the char + `(`).
        let shell = brush_core::Shell::builder().build().await.unwrap();
        for prefix in ["£", "€", "é", "½", "§", "²", "ï", "¤", "…"] {
            for line in [prefix.to_string(), format!("{prefix}(")] {
                let highlighted = highlight_command(&shell, &line, line.len());
                assert_spans_are_valid(&highlighted);
            }
        }
    }
}
