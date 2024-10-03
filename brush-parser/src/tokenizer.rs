use std::fmt::Display;
use utf8_chars::BufReadCharsExt;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) enum TokenEndReason {
    /// End of input was reached.
    EndOfInput,
    /// An unescaped newline char was reached.
    UnescapedNewLine,
    /// Specified terminating char.
    SpecifiedTerminatingChar,
    /// A non-newline blank char was reached.
    NonNewLineBlank,
    /// A non-newline token-delimiting char was encountered.
    Other,
}

/// Represents a position in a source shell script.
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
pub struct SourcePosition {
    /// The 0-based index of the character in the input stream.
    pub index: i32,
    /// The 1-based line number.
    pub line: i32,
    /// The 1-based column number.
    pub column: i32,
}

impl Display for SourcePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("line {} col {}", self.line, self.column))
    }
}

/// Represents the location of a token in its source shell script.
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
pub struct TokenLocation {
    /// The start position of the token.
    pub start: SourcePosition,
    /// The end position of the token (exclusive).
    pub end: SourcePosition,
}

/// Represents a token extracted from a shell script.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
pub enum Token {
    /// An operator token.
    Operator(String, TokenLocation),
    /// A word token.
    Word(String, TokenLocation),
}

impl Token {
    /// Returns the string value of the token.
    pub fn to_str(&self) -> &str {
        match self {
            Token::Operator(s, _) => s,
            Token::Word(s, _) => s,
        }
    }

    /// Returns the location of the token in the source script.
    pub fn location(&self) -> &TokenLocation {
        match self {
            Token::Operator(_, l) => l,
            Token::Word(_, l) => l,
        }
    }
}

/// Encapsulates the result of tokenizing a shell script.
#[derive(Clone, Debug)]
pub(crate) struct TokenizeResult {
    /// Reason for tokenization ending.
    pub reason: TokenEndReason,
    /// The token that was extracted, if any.
    pub token: Option<Token>,
}

/// Represents an error that occurred during tokenization.
#[derive(thiserror::Error, Debug)]
pub enum TokenizerError {
    /// An unterminated escape sequence was encountered at the end of the input stream.
    #[error("unterminated escape sequence")]
    UnterminatedEscapeSequence,

    /// An unterminated single-quoted substring was encountered at the end of the input stream.
    #[error("unterminated single quote at {0}")]
    UnterminatedSingleQuote(SourcePosition),

    /// An unterminated double-quoted substring was encountered at the end of the input stream.
    #[error("unterminated double quote at {0}")]
    UnterminatedDoubleQuote(SourcePosition),

    /// An unterminated back-quoted substring was encountered at the end of the input stream.
    #[error("unterminated backquote near {0}")]
    UnterminatedBackquote(SourcePosition),

    /// An unterminated extended glob (extglob) pattern was encountered at the end of the input
    /// stream.
    #[error("unterminated extglob near {0}")]
    UnterminatedExtendedGlob(SourcePosition),

    /// An unterminated variable expression was encountered at the end of the input stream.
    #[error("unterminated variable expression")]
    UnterminatedVariable,

    /// An unterminated command substitiion was encountered at the end of the input stream.
    #[error("unterminated command substitution")]
    UnterminatedCommandSubstitution,

    /// An error occurred decoding UTF-8 characters in the input stream.
    #[error("failed to decode UTF-8 characters")]
    FailedDecoding,

    /// An I/O here tag was missing.
    #[error("missing here tag for here document body")]
    MissingHereTagForDocumentBody,

    /// The indicated I/O here tag was missing.
    #[error("missing here tag '{0}'")]
    MissingHereTag(String),

    /// An unterminated here document sequence was encountered at the end of the input stream.
    #[error("unterminated here document sequence; tag(s) found at: [{0}]")]
    UnterminatedHereDocuments(String),

    /// An I/O error occurred while reading from the input stream.
    #[error("failed to read input")]
    ReadError(#[from] std::io::Error),

    /// An unimplemented tokenization feature was encountered.
    #[error("unimplemented tokenization: {0}")]
    Unimplemented(&'static str),
}

impl TokenizerError {
    pub fn is_incomplete(&self) -> bool {
        matches!(
            self,
            Self::UnterminatedEscapeSequence
                | Self::UnterminatedSingleQuote(_)
                | Self::UnterminatedDoubleQuote(_)
                | Self::UnterminatedBackquote(_)
                | Self::UnterminatedCommandSubstitution
                | Self::UnterminatedVariable
                | Self::UnterminatedExtendedGlob(_)
                | Self::UnterminatedHereDocuments(_)
        )
    }
}

/// Encapsulates a sequence of tokens.
#[derive(Debug)]
pub(crate) struct Tokens<'a> {
    /// Sequence of tokens.
    pub tokens: &'a [Token],
}

#[derive(Clone, Debug)]
enum QuoteMode {
    None,
    Single(SourcePosition),
    Double(SourcePosition),
}

#[derive(Clone, Debug)]
enum HereState {
    /// In this state, we are not currently tracking any here-documents.
    None,
    /// In this state, we expect that the next token will be a here tag.
    NextTokenIsHereTag { remove_tabs: bool },
    /// In this state, the *current* token is a here tag.
    CurrentTokenIsHereTag { remove_tabs: bool },
    /// In this state, we expect that the *next line* will be the body of
    /// a here-document.
    NextLineIsHereDoc,
    /// In this state, we are in the set of lines that comprise 1 or more
    /// consecutive here-document bodies.
    InHereDocs,
}

#[derive(Clone, Debug)]
struct HereTag {
    tag: String,
    remove_tabs: bool,
    position: SourcePosition,
    pending_tokens_after: Vec<TokenizeResult>,
}

#[derive(Clone, Debug)]
struct CrossTokenParseState {
    /// Cursor within the overall token stream; used for error reporting.
    cursor: SourcePosition,
    /// Current state of parsing here-documents.
    here_state: HereState,
    /// Ordered queue of here tags for which we're still looking for matching here-document bodies.
    current_here_tags: Vec<HereTag>,
    /// Tokens already tokenized that should be used first to serve requests for tokens.
    queued_tokens: Vec<TokenizeResult>,
    /// Are we in an arithmetic expansion?
    arithmetic_expansion: bool,
}

/// Options controlling how the tokenizer operates.
#[derive(Clone, Debug)]
pub struct TokenizerOptions {
    /// Whether or not to enable extended globbing patterns (extglob).
    pub enable_extended_globbing: bool,
    /// Whether or not to operate in POSIX compliance mode.
    pub posix_mode: bool,
}

impl Default for TokenizerOptions {
    fn default() -> Self {
        Self {
            enable_extended_globbing: true,
            posix_mode: false,
        }
    }
}

/// A tokenizer for shell scripts.
pub(crate) struct Tokenizer<'a, R: ?Sized + std::io::BufRead> {
    char_reader: std::iter::Peekable<utf8_chars::Chars<'a, R>>,
    cross_state: CrossTokenParseState,
    options: TokenizerOptions,
}

/// Encapsulates the current token parsing state.
#[derive(Clone, Debug)]
struct TokenParseState {
    pub start_position: SourcePosition,
    pub token_so_far: String,
    pub token_is_operator: bool,
    pub in_escape: bool,
    pub quote_mode: QuoteMode,
}

impl TokenParseState {
    pub fn new(start_position: &SourcePosition) -> Self {
        TokenParseState {
            start_position: start_position.clone(),
            token_so_far: String::new(),
            token_is_operator: false,
            in_escape: false,
            quote_mode: QuoteMode::None,
        }
    }

    pub fn pop(&mut self, end_position: &SourcePosition) -> Token {
        let token_location = TokenLocation {
            start: std::mem::take(&mut self.start_position),
            end: end_position.clone(),
        };

        let token = if std::mem::take(&mut self.token_is_operator) {
            Token::Operator(std::mem::take(&mut self.token_so_far), token_location)
        } else {
            Token::Word(std::mem::take(&mut self.token_so_far), token_location)
        };

        self.start_position = end_position.clone();
        self.in_escape = false;
        self.quote_mode = QuoteMode::None;

        token
    }

    pub fn started_token(&self) -> bool {
        !self.token_so_far.is_empty()
    }

    pub fn append_char(&mut self, c: char) {
        self.token_so_far.push(c);
    }

    pub fn append_str(&mut self, s: &str) {
        self.token_so_far.push_str(s);
    }

    pub fn unquoted(&self) -> bool {
        !self.in_escape && matches!(self.quote_mode, QuoteMode::None)
    }

    pub fn current_token(&self) -> &str {
        &self.token_so_far
    }

    pub fn is_specific_operator(&self, operator: &str) -> bool {
        self.token_is_operator && self.current_token() == operator
    }

    pub fn in_operator(&self) -> bool {
        self.token_is_operator
    }

    fn is_newline(&self) -> bool {
        self.token_so_far == "\n"
    }

    fn replace_with_here_doc(&mut self, s: String) {
        self.token_so_far = s;
    }

    pub fn delimit_current_token(
        &mut self,
        reason: TokenEndReason,
        cross_token_state: &mut CrossTokenParseState,
    ) -> Result<Option<TokenizeResult>, TokenizerError> {
        if !self.started_token() {
            return Ok(Some(TokenizeResult {
                reason,
                token: None,
            }));
        }

        // TODO: Make sure the here-tag meets criteria (and isn't a newline).
        match cross_token_state.here_state {
            HereState::NextTokenIsHereTag { remove_tabs } => {
                cross_token_state.here_state = HereState::CurrentTokenIsHereTag { remove_tabs };
            }
            HereState::CurrentTokenIsHereTag { remove_tabs } => {
                if self.is_newline() {
                    return Err(TokenizerError::MissingHereTag(
                        self.current_token().to_owned(),
                    ));
                }

                cross_token_state.here_state = HereState::NextLineIsHereDoc;

                if self.current_token().contains('\"')
                    || self.current_token().contains('\'')
                    || self.current_token().contains('\\')
                {
                    return Err(TokenizerError::Unimplemented("quoted or escaped here tag"));
                }

                // Include the \n in the here tag so it's easier to check against.
                cross_token_state.current_here_tags.push(HereTag {
                    tag: std::format!("\n{}\n", self.current_token()),
                    remove_tabs,
                    position: cross_token_state.cursor.clone(),
                    pending_tokens_after: vec![],
                });
            }
            HereState::NextLineIsHereDoc => {
                if self.is_newline() {
                    cross_token_state.here_state = HereState::InHereDocs;
                }

                // We need to queue it up for later so we can get the here-document
                // body to show up in the token stream right after the here tag.
                if let Some(last_here_tag) = cross_token_state.current_here_tags.last_mut() {
                    let token = self.pop(&cross_token_state.cursor);
                    let result = TokenizeResult {
                        reason,
                        token: Some(token),
                    };

                    last_here_tag.pending_tokens_after.push(result);
                } else {
                    return Err(TokenizerError::MissingHereTagForDocumentBody);
                }

                return Ok(None);
            }
            HereState::InHereDocs => {
                // We hit the end of the current here-document.
                let completed_here_tag = cross_token_state.current_here_tags.remove(0);

                // Now we're ready to start serving up any tokens that came between the completed
                // here tag and the next here tag (or newline after it if it was the last).
                for pending_token in completed_here_tag.pending_tokens_after {
                    cross_token_state.queued_tokens.push(pending_token);
                }

                if cross_token_state.current_here_tags.is_empty() {
                    cross_token_state.here_state = HereState::None;
                }
            }
            HereState::None => (),
        }

        let token = self.pop(&cross_token_state.cursor);
        let result = TokenizeResult {
            reason,
            token: Some(token),
        };

        Ok(Some(result))
    }
}

/// Break the given input shell script string into tokens, returning the tokens.
///
/// # Arguments
///
/// * `input` - The shell script to tokenize.
pub fn tokenize_str(input: &str) -> Result<Vec<Token>, TokenizerError> {
    cacheable_tokenize_str(input.to_owned())
}

#[cached::proc_macro::cached(size = 64, result = true)]
pub fn cacheable_tokenize_str(input: String) -> Result<Vec<Token>, TokenizerError> {
    let mut reader = std::io::BufReader::new(input.as_bytes());
    let mut tokenizer = crate::tokenizer::Tokenizer::new(&mut reader, &TokenizerOptions::default());

    let mut tokens = vec![];
    while let Some(token) = tokenizer.next_token()?.token {
        tokens.push(token);
    }

    Ok(tokens)
}

impl<'a, R: ?Sized + std::io::BufRead> Tokenizer<'a, R> {
    pub fn new(reader: &'a mut R, options: &TokenizerOptions) -> Tokenizer<'a, R> {
        Tokenizer {
            options: options.clone(),
            char_reader: reader.chars().peekable(),
            cross_state: CrossTokenParseState {
                cursor: SourcePosition {
                    index: 0,
                    line: 1,
                    column: 1,
                },
                here_state: HereState::None,
                current_here_tags: vec![],
                queued_tokens: vec![],
                arithmetic_expansion: false,
            },
        }
    }

    pub fn current_location(&self) -> Option<SourcePosition> {
        Some(self.cross_state.cursor.clone())
    }

    fn next_char(&mut self) -> Result<Option<char>, TokenizerError> {
        let c = self
            .char_reader
            .next()
            .transpose()
            .map_err(TokenizerError::ReadError)?;

        if let Some(ch) = c {
            if ch == '\n' {
                self.cross_state.cursor.line += 1;
                self.cross_state.cursor.column = 1;
            } else {
                self.cross_state.cursor.column += 1;
            }
            self.cross_state.cursor.index += 1;
        }

        Ok(c)
    }

    fn consume_char(&mut self) -> Result<(), TokenizerError> {
        let _ = self.next_char()?;
        Ok(())
    }

    fn peek_char(&mut self) -> Result<Option<char>, TokenizerError> {
        match self.char_reader.peek() {
            Some(result) => match result {
                Ok(c) => Ok(Some(*c)),
                Err(_) => Err(TokenizerError::FailedDecoding),
            },
            None => Ok(None),
        }
    }

    pub fn next_token(&mut self) -> Result<TokenizeResult, TokenizerError> {
        self.next_token_until(None)
    }

    #[allow(clippy::if_same_then_else)]
    fn next_token_until(
        &mut self,
        terminating_char: Option<char>,
    ) -> Result<TokenizeResult, TokenizerError> {
        // First satisfy token results from our queue. Once we exhaust the queue then
        // we'll look at the input stream.
        if !self.cross_state.queued_tokens.is_empty() {
            return Ok(self.cross_state.queued_tokens.remove(0));
        }

        let mut state = TokenParseState::new(&self.cross_state.cursor);
        let mut result: Option<TokenizeResult> = None;

        while result.is_none() {
            let next = self.peek_char()?;
            let c = next.unwrap_or('\0');

            // When we hit the end of the input, then we're done with the current token (if there is
            // one).
            if next.is_none() {
                // TODO: Verify we're not waiting on some terminating character?
                // Verify we're out of all quotes.
                if state.in_escape {
                    return Err(TokenizerError::UnterminatedEscapeSequence);
                }
                match state.quote_mode {
                    QuoteMode::None => (),
                    QuoteMode::Single(pos) => {
                        return Err(TokenizerError::UnterminatedSingleQuote(pos));
                    }
                    QuoteMode::Double(pos) => {
                        return Err(TokenizerError::UnterminatedDoubleQuote(pos));
                    }
                }

                // Verify we're not in a here document.
                if !matches!(self.cross_state.here_state, HereState::None) {
                    let tag_positions = self
                        .cross_state
                        .current_here_tags
                        .iter()
                        .map(|tag| std::format!("{}", tag.position))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(TokenizerError::UnterminatedHereDocuments(tag_positions));
                }

                result = state
                    .delimit_current_token(TokenEndReason::EndOfInput, &mut self.cross_state)?;
            //
            // Look for the specially specified terminating char.
            //
            } else if state.unquoted() && terminating_char == Some(c) {
                result = state.delimit_current_token(
                    TokenEndReason::SpecifiedTerminatingChar,
                    &mut self.cross_state,
                )?;
            //
            // Handle being in a here document.
            //
            } else if matches!(self.cross_state.here_state, HereState::InHereDocs) {
                //
                // For now, just include the character in the current token. We also check
                // if there are leading tabs to be removed.
                //
                if !self.cross_state.current_here_tags.is_empty()
                    && self.cross_state.current_here_tags[0].remove_tabs
                    && (!state.started_token() || state.current_token().ends_with('\n'))
                    && c == '\t'
                {
                    // Consume it but don't include it.
                    self.consume_char()?;
                } else {
                    self.consume_char()?;
                    state.append_char(c);

                    let without_suffix = state
                        .current_token()
                        .strip_suffix(self.cross_state.current_here_tags[0].tag.as_str())
                        .map(|s| s.to_owned());

                    if let Some(mut without_suffix) = without_suffix {
                        without_suffix.push('\n');

                        state.replace_with_here_doc(without_suffix);

                        result = state
                            .delimit_current_token(TokenEndReason::Other, &mut self.cross_state)?;
                    }
                }
            } else if state.in_operator() {
                //
                // We're in an operator. See if this character continues an operator, or if it
                // must be a separate token (because it wouldn't make a prefix of an operator).
                //

                let mut hypothetical_token = state.current_token().to_owned();
                hypothetical_token.push(c);

                if state.unquoted() && self.is_operator(hypothetical_token.as_ref()) {
                    self.consume_char()?;
                    state.append_char(c);
                } else {
                    assert!(state.started_token());

                    //
                    // N.B. If the completed operator indicates a here-document, then keep
                    // track that the *next* token should be the here-tag.
                    //
                    if self.cross_state.arithmetic_expansion {
                        // Nothing to do; we're in an arithmetic expansion so << and <<-
                        // are not here-docs, they're either a left-shift operator or
                        // a left-shift operator followed by a unary minus operator.
                    } else if state.is_specific_operator("<<") {
                        self.cross_state.here_state =
                            HereState::NextTokenIsHereTag { remove_tabs: false };
                    } else if state.is_specific_operator("<<-") {
                        self.cross_state.here_state =
                            HereState::NextTokenIsHereTag { remove_tabs: true };
                    }

                    result = state
                        .delimit_current_token(TokenEndReason::Other, &mut self.cross_state)?;
                }
            //
            // See if this is a character that changes the current escaping/quoting state.
            //
            } else if does_char_newly_affect_quoting(&state, c) {
                if c == '\\' {
                    // Consume the backslash ourselves so we can peek past it.
                    self.consume_char()?;

                    if matches!(self.peek_char()?, Some('\n')) {
                        // Make sure the newline char gets consumed too.
                        self.consume_char()?;

                        // Make sure to include neither the backslash nor the newline character.
                    } else {
                        state.in_escape = true;
                        state.append_char(c);
                    }
                } else if c == '\'' {
                    state.quote_mode = QuoteMode::Single(self.cross_state.cursor.clone());
                    self.consume_char()?;
                    state.append_char(c);
                } else if c == '\"' {
                    state.quote_mode = QuoteMode::Double(self.cross_state.cursor.clone());
                    self.consume_char()?;
                    state.append_char(c);
                }
            }
            //
            // Handle end of single-quote or double-quote.
            else if !state.in_escape
                && matches!(state.quote_mode, QuoteMode::Single(_))
                && c == '\''
            {
                state.quote_mode = QuoteMode::None;
                self.consume_char()?;
                state.append_char(c);
            } else if !state.in_escape
                && matches!(state.quote_mode, QuoteMode::Double(_))
                && c == '\"'
            {
                state.quote_mode = QuoteMode::None;
                self.consume_char()?;
                state.append_char(c);
            }
            //
            // Handle end of escape sequence.
            // TODO: Handle double-quote specific escape sequences.
            else if state.in_escape {
                state.in_escape = false;
                self.consume_char()?;
                state.append_char(c);
            } else if (state.unquoted()
                || (matches!(state.quote_mode, QuoteMode::Double(_)) && !state.in_escape))
                && (c == '$' || c == '`')
            {
                // TODO: handle quoted $ or ` in a double quote
                if c == '$' {
                    // Consume the '$' so we can peek beyond.
                    self.consume_char()?;

                    // Now peek beyond to see what we have.
                    let char_after_dollar_sign = self.peek_char()?;
                    match char_after_dollar_sign {
                        Some('(') => {
                            // Add the '$' we already consumed to the token.
                            state.append_char('$');

                            // Consume the '(' and add it to the token.
                            state.append_char(self.next_char()?.unwrap());

                            // Check to see if this is possibly an arithmetic expression
                            // (i.e., one that starts with `$((`).
                            let mut required_end_parens = 1;
                            if matches!(self.peek_char()?, Some('(')) {
                                // Consume the second '(' and add it to the token.
                                state.append_char(self.next_char()?.unwrap());
                                // Keep track that we'll need to see *2* end parentheses
                                // to leave this construct.
                                required_end_parens = 2;
                                // Keep track that we're in an arithmetic expression, since
                                // some text will be interpreted differently as a result
                                // (e.g., << is a left shift operator and not a here doc
                                // input redirection operator).
                                self.cross_state.arithmetic_expansion = true;
                            }

                            loop {
                                let cur_token = self.next_token_until(Some(')'))?;
                                if let Some(cur_token_value) = cur_token.token {
                                    state.append_str(cur_token_value.to_str());

                                    // If we encounter an embedded open parenthesis, then note that
                                    // we'll have to see the matching end to it before we worry
                                    // about the end of the
                                    // containing construct.
                                    if matches!(cur_token_value, Token::Operator(o, _) if o == "(")
                                    {
                                        required_end_parens += 1;
                                    }
                                }

                                match cur_token.reason {
                                    TokenEndReason::UnescapedNewLine
                                    | TokenEndReason::NonNewLineBlank => {
                                        state.append_char(' ');
                                    }
                                    TokenEndReason::SpecifiedTerminatingChar => {
                                        // We hit the ')' we were looking for. If this is the last
                                        // end parenthesis we needed to find, then we'll exit the
                                        // loop and consume
                                        // and append it.
                                        required_end_parens -= 1;
                                        if required_end_parens == 0 {
                                            break;
                                        }

                                        // This wasn't the *last* end parenthesis char, so let's
                                        // consume and append it here before we loop around again.
                                        state.append_char(self.next_char()?.unwrap());
                                    }
                                    TokenEndReason::EndOfInput => {
                                        return Err(TokenizerError::UnterminatedCommandSubstitution)
                                    }
                                    TokenEndReason::Other => (),
                                }
                            }

                            self.cross_state.arithmetic_expansion = false;

                            state.append_char(self.next_char()?.unwrap());
                        }

                        Some('{') => {
                            // Add the '$' we already consumed to the token.
                            state.append_char('$');

                            // Consume the '{' and add it to the token.
                            state.append_char(self.next_char()?.unwrap());

                            loop {
                                let cur_token = self.next_token_until(Some('}'))?;
                                if let Some(cur_token_value) = cur_token.token {
                                    state.append_str(cur_token_value.to_str())
                                }

                                if matches!(cur_token.reason, TokenEndReason::NonNewLineBlank) {
                                    state.append_char(' ');
                                }

                                match cur_token.reason {
                                    TokenEndReason::SpecifiedTerminatingChar => {
                                        // We hit the end brace we were looking for but did not
                                        // yet consume it. Do so now.
                                        state.append_char(self.next_char()?.unwrap());
                                        break;
                                    }
                                    TokenEndReason::EndOfInput => {
                                        return Err(TokenizerError::UnterminatedVariable)
                                    }
                                    TokenEndReason::UnescapedNewLine
                                    | TokenEndReason::NonNewLineBlank
                                    | TokenEndReason::Other => (),
                                }
                            }
                        }
                        _ => {
                            // This is either a different character, or else the end of the string.
                            // Either way, add the '$' we already consumed to the token.
                            state.append_char('$');
                        }
                    }
                } else {
                    // We look for the terminating backquote. First disable normal consumption and
                    // consume the starting backquote.
                    let backquote_pos = self.cross_state.cursor.clone();
                    self.consume_char()?;

                    // Add the opening backquote to the token.
                    state.append_char(c);

                    // Now continue until we see an unescaped backquote.
                    let mut escaping_enabled = false;
                    let mut done = false;
                    while !done {
                        // Read (and consume) the next char.
                        let next_char_in_backquote = self.next_char()?;
                        if let Some(cib) = next_char_in_backquote {
                            // Include it in the token no matter what.
                            state.append_char(cib);

                            // Watch out for escaping.
                            if !escaping_enabled && cib == '\\' {
                                escaping_enabled = true;
                            } else {
                                // Look for an unescaped backquote to terminate.
                                if !escaping_enabled && cib == '`' {
                                    done = true;
                                }
                                escaping_enabled = false;
                            }
                        } else {
                            return Err(TokenizerError::UnterminatedBackquote(backquote_pos));
                        }
                    }
                }
            }
            //
            // [Extension]
            // If extended globbing is enabled, the last consumed character is an
            // unquoted start of an extglob pattern, *and* if the current character
            // is an open parenthesis, then this begins an extglob pattern.
            else if c == '('
                && self.options.enable_extended_globbing
                && state.unquoted()
                && !state.in_operator()
                && state
                    .current_token()
                    .ends_with(|x| self.can_start_extglob(x))
            {
                // Consume the '(' and append it.
                self.consume_char()?;
                state.append_char(c);

                let mut paren_depth = 1;

                // Keep consuming until we see the matching end ')'.
                while paren_depth > 0 {
                    if let Some(extglob_char) = self.next_char()? {
                        // Include it in the token.
                        state.append_char(extglob_char);

                        // Look for ')' to terminate.
                        // TODO: handle escaping?
                        if extglob_char == '(' {
                            paren_depth += 1;
                        } else if extglob_char == ')' {
                            paren_depth -= 1;
                        }
                    } else {
                        return Err(TokenizerError::UnterminatedExtendedGlob(
                            self.cross_state.cursor.clone(),
                        ));
                    }
                }
            //
            // If the character *can* start an operator, then it will.
            //
            } else if state.unquoted() && self.can_start_operator(c) {
                if state.started_token() {
                    result = state
                        .delimit_current_token(TokenEndReason::Other, &mut self.cross_state)?;
                } else {
                    state.token_is_operator = true;
                    self.consume_char()?;
                    state.append_char(c);
                }
            //
            // Whitespace gets discarded (and delimits tokens).
            //
            } else if state.unquoted() && is_blank(c) {
                if state.started_token() {
                    result = state.delimit_current_token(
                        TokenEndReason::NonNewLineBlank,
                        &mut self.cross_state,
                    )?;
                } else {
                    // Make sure we don't include this char in the token range.
                    state.start_position.column += 1;
                    state.start_position.index += 1;
                }

                self.consume_char()?;
            }
            //
            // N.B. We need to remember if we were recursively called, say in a command
            // substitution; in that case we won't think a token was started but... we'd
            // be wrong.
            else if !state.token_is_operator
                && (state.started_token() || terminating_char.is_some())
            {
                self.consume_char()?;
                state.append_char(c);
            } else if c == '#' {
                // Consume the '#'.
                self.consume_char()?;

                let mut done = false;
                while !done {
                    done = match self.peek_char()? {
                        Some('\n') => true,
                        None => true,
                        _ => {
                            // Consume the peeked char; it's part of the comment.
                            self.consume_char()?;
                            false
                        }
                    };
                }

                // Re-start loop as if the comment never happened.
                continue;
            //
            // In all other cases where we have an in-progress token, we delimit here.
            //
            } else if state.started_token() {
                result =
                    state.delimit_current_token(TokenEndReason::Other, &mut self.cross_state)?;
            } else {
                // If we got here, then we don't have a token in progress and we're not starting an
                // operator. Add the character to a new token.
                self.consume_char()?;
                state.append_char(c);
            }
        }

        let result = result.unwrap();

        Ok(result)
    }

    fn can_start_extglob(&self, c: char) -> bool {
        matches!(c, '@' | '!' | '?' | '+' | '*')
    }

    fn can_start_operator(&self, c: char) -> bool {
        matches!(c, '&' | '(' | ')' | ';' | '\n' | '|' | '<' | '>')
    }

    fn is_operator(&self, s: &str) -> bool {
        // Handle non-POSIX operators.
        if !self.options.posix_mode && matches!(s, "<<<" | "&>" | "&>>") {
            return true;
        }

        matches!(
            s,
            "&" | "&&"
                | "("
                | ")"
                | ";"
                | ";;"
                | "\n"
                | "|"
                | "||"
                | "<"
                | ">"
                | ">|"
                | "<<"
                | ">>"
                | "<&"
                | ">&"
                | "<<-"
                | "<>"
        )
    }
}

impl<'a, R: ?Sized + std::io::BufRead> Iterator for Tokenizer<'a, R> {
    type Item = Result<TokenizeResult, TokenizerError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_token() {
            #[allow(clippy::manual_map)]
            Ok(result) => match result.token {
                Some(_) => Some(Ok(result)),
                None => None,
            },
            Err(e) => Some(Err(e)),
        }
    }
}

fn is_blank(c: char) -> bool {
    c == ' ' || c == '\t'
}

fn does_char_newly_affect_quoting(state: &TokenParseState, c: char) -> bool {
    // If we're currently escaped, then nothing affects quoting.
    if state.in_escape {
        return false;
    }

    match state.quote_mode {
        // When we're in a double quote, only a subset of escape sequences are recognized.
        QuoteMode::Double(_) => {
            if c == '\\' {
                // TODO: handle backslash in double quote
                true
            } else {
                false
            }
        }
        // When we're in a single quote, nothing affects quoting.
        QuoteMode::Single(_) => false,
        // When we're not already in a quote, then we can straightforwardly look for a
        // quote mark or backslash.
        QuoteMode::None => is_quoting_char(c),
    }
}

fn is_quoting_char(c: char) -> bool {
    matches!(c, '\\' | '\'' | '\"')
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use assert_matches::assert_matches;

    #[test]
    fn tokenize_empty() -> Result<()> {
        let tokens = tokenize_str("")?;
        assert_eq!(tokens.len(), 0);
        Ok(())
    }

    #[test]
    fn tokenize_line_continuation() -> Result<()> {
        let tokens = tokenize_str(
            r"a\
bc",
        )?;
        assert_matches!(
            &tokens[..],
            [t1 @ Token::Word(_, _)] if t1.to_str() == "abc"
        );
        Ok(())
    }

    #[test]
    fn tokenize_operators() -> Result<()> {
        assert_matches!(
            &tokenize_str("a>>b")?[..],
            [t1 @ Token::Word(_, _), t2 @ Token::Operator(_, _), t3 @ Token::Word(_, _)] if
                t1.to_str() == "a" &&
                t2.to_str() == ">>" &&
                t3.to_str() == "b"
        );
        Ok(())
    }

    #[test]
    fn tokenize_comment() -> Result<()> {
        let tokens = tokenize_str(
            r#"a #comment
"#,
        )?;
        assert_matches!(
            &tokens[..],
            [t1 @ Token::Word(_, _), t2 @ Token::Operator(_, _)] if
                t1.to_str() == "a" &&
                t2.to_str() == "\n"
        );
        Ok(())
    }

    #[test]
    fn tokenize_comment_at_eof() -> Result<()> {
        assert_matches!(
            &tokenize_str(r#"a #comment"#)?[..],
            [t1 @ Token::Word(_, _)] if t1.to_str() == "a"
        );
        Ok(())
    }

    #[test]
    fn tokenize_here_doc() -> Result<()> {
        let tokens = tokenize_str(
            r#"cat <<HERE
SOMETHING
HERE
echo after
"#,
        )?;
        assert_matches!(
            &tokens[..],
            [t1 @ Token::Word(_, _),
             t2 @ Token::Operator(_, _),
             t3 @ Token::Word(_, _),
             t4 @ Token::Word(_, _),
             t5 @ Token::Operator(_, _),
             t6 @ Token::Word(_, _),
             t7 @ Token::Word(_, _),
             t8 @ Token::Operator(_, _)] if
                t1.to_str() == "cat" &&
                t2.to_str() == "<<" &&
                t3.to_str() == "HERE" &&
                t4.to_str() == "SOMETHING\n" &&
                t5.to_str() == "\n" &&
                t6.to_str() == "echo" &&
                t7.to_str() == "after" &&
                t8.to_str() == "\n"
        );
        Ok(())
    }

    #[test]
    fn tokenize_here_doc_with_tab_removal() -> Result<()> {
        let tokens = tokenize_str(
            r#"cat <<-HERE
	SOMETHING
	HERE
"#,
        )?;
        assert_matches!(
            &tokens[..],
            [t1 @ Token::Word(_, _),
             t2 @ Token::Operator(_, _),
             t3 @ Token::Word(_, _),
             t4 @ Token::Word(_, _),
             t5 @ Token::Operator(_, _)] if
                t1.to_str() == "cat" &&
                t2.to_str() == "<<-" &&
                t3.to_str() == "HERE" &&
                t4.to_str() == "SOMETHING\n" &&
                t5.to_str() == "\n"
        );
        Ok(())
    }

    #[test]
    fn tokenize_unterminated_here_doc() -> Result<()> {
        let result = tokenize_str(
            r#"cat <<HERE
SOMETHING
"#,
        );
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn tokenize_missing_here_tag() -> Result<()> {
        let result = tokenize_str(
            r"cat <<
",
        );
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn tokenize_simple_backquote() -> Result<()> {
        assert_matches!(
            &tokenize_str(r#"echo `echo hi`"#)?[..],
            [t1 @ Token::Word(_, _), t2 @ Token::Word(_, _)] if
                t1.to_str() == "echo" &&
                t2.to_str() == "`echo hi`"
        );
        Ok(())
    }

    #[test]
    fn tokenize_backquote_with_escape() -> Result<()> {
        assert_matches!(
            &tokenize_str(r"echo `echo\`hi`")?[..],
            [t1 @ Token::Word(_, _), t2 @ Token::Word(_, _)] if
                t1.to_str() == "echo" &&
                t2.to_str() == r"`echo\`hi`"
        );
        Ok(())
    }

    #[test]
    fn tokenize_unterminated_backquote() {
        assert_matches!(
            tokenize_str("`"),
            Err(TokenizerError::UnterminatedBackquote(_))
        );
    }

    #[test]
    fn tokenize_unterminated_command_substitution() {
        assert_matches!(
            tokenize_str("$("),
            Err(TokenizerError::UnterminatedCommandSubstitution)
        );
    }

    #[test]
    fn tokenize_command_substitution() -> Result<()> {
        assert_matches!(
            &tokenize_str("a$(echo hi)b c")?[..],
            [t1 @ Token::Word(_, _), t2 @ Token::Word(_, _)] if
                t1.to_str() == "a$(echo hi)b" &&
                t2.to_str() == "c"
        );
        Ok(())
    }

    #[test]
    fn tokenize_command_substitution_containing_extglob() -> Result<()> {
        assert_matches!(
            &tokenize_str("echo $(echo !(x))")?[..],
            [t1 @ Token::Word(_, _), t2 @ Token::Word(_, _)] if
                t1.to_str() == "echo" &&
                t2.to_str() == "$(echo !(x))"
        );
        Ok(())
    }

    #[test]
    fn tokenize_arithmetic_expression() -> Result<()> {
        assert_matches!(
            &tokenize_str("a$((1+2))b c")?[..],
            [t1 @ Token::Word(_, _), t2 @ Token::Word(_, _)] if
                t1.to_str() == "a$((1+2))b" &&
                t2.to_str() == "c"
        );
        Ok(())
    }

    #[test]
    fn tokenize_arithmetic_expression_with_space() -> Result<()> {
        // N.B. The spacing comes out a bit odd, but it gets processed okay
        // by later stages.
        assert_matches!(
            &tokenize_str("$(( 1 ))")?[..],
            [t1 @ Token::Word(_, _)] if
                t1.to_str() == "$((1 ))"
        );
        Ok(())
    }
    #[test]
    fn tokenize_arithmetic_expression_with_parens() -> Result<()> {
        assert_matches!(
            &tokenize_str("$(( (0) ))")?[..],
            [t1 @ Token::Word(_, _)] if
                t1.to_str() == "$(((0)))"
        );
        Ok(())
    }

    #[test]
    fn tokenize_special_parameters() -> Result<()> {
        assert_matches!(
            &tokenize_str("$$")?[..],
            [t1 @ Token::Word(_, _)] if t1.to_str() == "$$"
        );
        assert_matches!(
            &tokenize_str("$@")?[..],
            [t1 @ Token::Word(_, _)] if t1.to_str() == "$@"
        );
        assert_matches!(
            &tokenize_str("$!")?[..],
            [t1 @ Token::Word(_, _)] if t1.to_str() == "$!"
        );
        assert_matches!(
            &tokenize_str("$?")?[..],
            [t1 @ Token::Word(_, _)] if t1.to_str() == "$?"
        );
        assert_matches!(
            &tokenize_str("$*")?[..],
            [t1 @ Token::Word(_, _)] if t1.to_str() == "$*"
        );
        Ok(())
    }

    #[test]
    fn tokenize_unbraced_parameter_expansion() -> Result<()> {
        assert_matches!(
            &tokenize_str("$x")?[..],
            [t1 @ Token::Word(_, _)] if t1.to_str() == "$x"
        );
        assert_matches!(
            &tokenize_str("a$x")?[..],
            [t1 @ Token::Word(_, _)] if t1.to_str() == "a$x"
        );
        Ok(())
    }

    #[test]
    fn tokenize_unterminated_parameter_expansion() {
        assert_matches!(
            tokenize_str("${x"),
            Err(TokenizerError::UnterminatedVariable)
        );
    }

    #[test]
    fn tokenize_braced_parameter_expansion() -> Result<()> {
        assert_matches!(
            &tokenize_str("${x}")?[..],
            [t1 @ Token::Word(_, _)] if t1.to_str() == "${x}"
        );
        assert_matches!(
            &tokenize_str("a${x}b")?[..],
            [t1 @ Token::Word(_, _)] if t1.to_str() == "a${x}b"
        );
        Ok(())
    }

    #[test]
    fn tokenize_braced_parameter_expansion_with_escaping() -> Result<()> {
        assert_matches!(
            &tokenize_str(r"a${x\}}b")?[..],
            [t1 @ Token::Word(_, _)] if t1.to_str() == r"a${x\}}b"
        );
        Ok(())
    }

    #[test]
    fn tokenize_whitespace() -> Result<()> {
        assert_matches!(
            &tokenize_str("1 2 3")?[..],
            [t1 @ Token::Word(_, _), t2 @ Token::Word(_, _), t3 @ Token::Word(_, _)] if
                t1.to_str() == "1" &&
                t2.to_str() == "2" &&
                t3.to_str() == "3"
        );
        Ok(())
    }

    #[test]
    fn tokenize_escaped_whitespace() -> Result<()> {
        assert_matches!(
            &tokenize_str(r"1\ 2 3")?[..],
            [t1 @ Token::Word(_, _), t2 @ Token::Word(_, _)] if
                t1.to_str() == r"1\ 2" &&
                t2.to_str() == "3"
        );
        Ok(())
    }

    #[test]
    fn tokenize_single_quote() -> Result<()> {
        assert_matches!(
            &tokenize_str(r"x'a b'y")?[..],
            [t1 @ Token::Word(_, _)] if
                t1.to_str() == r"x'a b'y"
        );
        Ok(())
    }

    #[test]
    fn tokenize_double_quote() -> Result<()> {
        assert_matches!(
            &tokenize_str(r#"x"a b"y"#)?[..],
            [t1 @ Token::Word(_, _)] if
                t1.to_str() == r#"x"a b"y"#
        );
        Ok(())
    }

    #[test]
    fn tokenize_double_quoted_command_substitution() -> Result<()> {
        assert_matches!(
            &tokenize_str(r#"x"$(echo hi)"y"#)?[..],
            [t1 @ Token::Word(_, _)] if
                t1.to_str() == r#"x"$(echo hi)"y"#
        );
        Ok(())
    }

    #[test]
    fn tokenize_double_quoted_arithmetic_expression() -> Result<()> {
        assert_matches!(
            &tokenize_str(r#"x"$((1+2))"y"#)?[..],
            [t1 @ Token::Word(_, _)] if
                t1.to_str() == r#"x"$((1+2))"y"#
        );
        Ok(())
    }
}
