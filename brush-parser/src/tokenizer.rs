use std::borrow::Cow;
use std::sync::Arc;
use utf8_chars::BufReadCharsExt;

use crate::{SourcePosition, SourceSpan};

#[derive(Clone, Debug)]
pub(crate) enum TokenEndReason {
    /// End of input was reached.
    EndOfInput,
    /// An unescaped newline char was reached.
    UnescapedNewLine,
    /// The next char (not yet consumed) closes the nested expansion being tokenized. Never
    /// carries a token, so here-document bookkeeping can't hold it back.
    ExpansionClosingChar,
    /// A non-newline blank char was reached.
    NonNewLineBlank,
    /// A here-document's body is starting.
    HereDocumentBodyStart,
    /// A here-document's body was terminated.
    HereDocumentBodyEnd,
    /// A here-document's end tag was reached.
    HereDocumentEndTag,
    /// An operator was started.
    OperatorStart,
    /// An operator was terminated.
    OperatorEnd,
    /// Some other condition was reached.
    Other,
}

/// Compatibility alias for `SourceSpan`.
pub type TokenLocation = SourceSpan;

/// Represents a token extracted from a shell script.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[cfg_attr(
    any(test, feature = "serde"),
    derive(PartialEq, Eq, serde::Serialize, serde::Deserialize)
)]
pub enum Token {
    /// An operator token.
    Operator(String, SourceSpan),
    /// A word token.
    Word(String, SourceSpan),
}

impl Token {
    /// Returns the string value of the token.
    pub fn to_str(&self) -> &str {
        match self {
            Self::Operator(s, _) => s,
            Self::Word(s, _) => s,
        }
    }

    /// Returns the location of the token in the source script.
    pub const fn location(&self) -> &SourceSpan {
        match self {
            Self::Operator(_, l) => l,
            Self::Word(_, l) => l,
        }
    }
}

#[cfg(feature = "diagnostics")]
impl From<&Token> for miette::SourceSpan {
    fn from(token: &Token) -> Self {
        let start = token.location().start.as_ref();
        Self::new(start.into(), token.location().length())
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

    /// An unterminated ANSI C-quoted substring was encountered at the end of the input stream.
    #[error("unterminated ANSI C quote at {0}")]
    UnterminatedAnsiCQuote(SourcePosition),

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

    /// An unterminated arithmetic or other expansion was encountered at the end of the input
    /// stream.
    #[error("unterminated expansion")]
    UnterminatedExpansion,

    /// An error occurred decoding UTF-8 characters in the input stream.
    #[error("failed to decode UTF-8 characters")]
    FailedDecoding,

    /// A here-document body was due with no here tag recorded for it. This reflects an
    /// inconsistency in the tokenizer's own state, not a problem with the input.
    #[error("missing here tag for here document body")]
    MissingHereTagForDocumentBody,

    /// A here-document operator (`<<` or `<<-`) was followed by an operator (e.g., a newline,
    /// `;` or `)`) where its tag belongs; the string is that operator.
    #[error("missing here-document tag before {0:?}")]
    MissingHereTag(String),

    /// The input ended after a here-document operator (`<<` or `<<-`) and before its tag, but
    /// not right at the operator (e.g., after blanks or a line continuation following it). Input
    /// ending right at the operator tokenizes, leaving the parser to report its end. Unlike
    /// [`TokenizerError::MissingHereTag`], more input could still supply the tag.
    #[error("missing here-document tag at end of input")]
    MissingHereTagAtEndOfInput,

    /// An unterminated here document sequence was encountered at the end of the input stream.
    #[error("unterminated here document; tag(s) {0} declared at {1}")]
    UnterminatedHereDocuments(String, String),

    /// Expansions delimited by brackets (`$(...)`, `$((...))`, `$[...]` and `${...}`) were
    /// nested in one another more deeply than the tokenizer supports.
    #[error("expansions nested too deeply (limit: {})", MAX_EXPANSION_NESTING)]
    ExpansionNestingTooDeep,

    /// An I/O error occurred while reading from the input stream.
    #[error("failed to read input")]
    ReadError(#[from] std::io::Error),
}

impl TokenizerError {
    /// Returns true if the error represents an error that could possibly be due
    /// to an incomplete input stream.
    pub const fn is_incomplete(&self) -> bool {
        matches!(
            self,
            Self::UnterminatedEscapeSequence
                | Self::UnterminatedAnsiCQuote(..)
                | Self::UnterminatedSingleQuote(..)
                | Self::UnterminatedDoubleQuote(..)
                | Self::UnterminatedBackquote(..)
                | Self::UnterminatedCommandSubstitution
                | Self::UnterminatedExpansion
                | Self::UnterminatedVariable
                | Self::UnterminatedExtendedGlob(..)
                | Self::UnterminatedHereDocuments(..)
                | Self::MissingHereTagAtEndOfInput
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
    AnsiC(SourcePosition),
    Single(SourcePosition),
    Double(SourcePosition),
}

#[derive(Clone, Debug, Default)]
enum HereState {
    /// In this state, we are not currently tracking any here-documents.
    #[default]
    None,
    /// In this state, a here-document operator is being delimited, and the next token will be
    /// its tag.
    NextTokenIsHereTag { remove_tabs: bool },
    /// In this state, the operator has been read, and the token now being read (once it ends)
    /// will be its tag.
    CurrentTokenIsHereTag {
        remove_tabs: bool,
        operator_token_result: TokenizeResult,
    },
    /// In this state, we expect that the *next line* will be the body of
    /// a here-document.
    NextLineIsHereDoc,
    /// In this state, we are in the set of lines that comprise 1 or more
    /// consecutive here-document bodies.
    InHereDocs,
}

impl HereState {
    /// Returns true if a here-document operator has been read but its tag hasn't (yet).
    const fn awaiting_tag(&self) -> bool {
        matches!(
            self,
            Self::NextTokenIsHereTag { .. } | Self::CurrentTokenIsHereTag { .. }
        )
    }
}

#[derive(Clone, Debug)]
struct HereTag {
    tag: String,
    tag_was_escaped_or_quoted: bool,
    remove_tabs: bool,
    position: SourcePosition,
    tokens: Vec<TokenizeResult>,
    pending_tokens_after: Vec<TokenizeResult>,
}

#[derive(Clone, Debug)]
struct CrossTokenParseState {
    /// Cursor within the overall token stream; used for error reporting.
    cursor: SourcePosition,
    /// Tokens already tokenized that should be used first to serve requests for tokens.
    queued_tokens: Vec<TokenizeResult>,
    /// State of the expansion frame being tokenized; see [`ExpansionFrame`].
    frame: ExpansionFrame,
}

/// An expansion delimited by brackets, whose contents are tokenized recursively (as a
/// [`ExpansionFrame`] of their own) while it's consumed as part of a word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NestedExpansion {
    /// `$(...)`
    CommandSubstitution,
    /// `$((...))`
    ArithmeticExpansion,
    /// `$[...]`
    LegacyArithmeticExpansion,
    /// `${...}`
    ParameterExpansion,
}

impl NestedExpansion {
    /// Returns the character that closes the expansion.
    const fn closing_char(self) -> char {
        match self {
            Self::CommandSubstitution | Self::ArithmeticExpansion => ')',
            Self::LegacyArithmeticExpansion => ']',
            Self::ParameterExpansion => '}',
        }
    }

    /// Returns whether the expansion's contents are arithmetic, where `<<` is a shift.
    const fn is_arithmetic(self) -> bool {
        matches!(
            self,
            Self::ArithmeticExpansion | Self::LegacyArithmeticExpansion
        )
    }

    /// Returns the character that, in the expansion's contents, pairs with its closing char:
    /// the `(` of `$( (a) )`, or the `[` of `$[ a[1] ]`. Bash doesn't pair braces in `${...}`.
    const fn opening_char(self) -> Option<char> {
        match self {
            Self::CommandSubstitution | Self::ArithmeticExpansion => Some('('),
            Self::LegacyArithmeticExpansion => Some('['),
            Self::ParameterExpansion => None,
        }
    }

    /// Returns how many closing chars end the expansion, counting from just after its
    /// opening: two for `$((`, whose opening leaves two parens open, and one otherwise.
    const fn closers_after_opening(self) -> usize {
        match self {
            Self::ArithmeticExpansion => 2,
            _ => 1,
        }
    }

    /// Returns the error for input that ends before the expansion is closed.
    const fn unterminated_error(self) -> TokenizerError {
        match self {
            Self::ParameterExpansion => TokenizerError::UnterminatedVariable,
            _ => TokenizerError::UnterminatedExpansion,
        }
    }
}

/// Tokenizer state belonging to one frame of the stack of nested expansions being tokenized;
/// the bottom frame is the top level of the input, outside any expansion. Consuming a nested
/// expansion pushes a fresh frame for its contents and pops it at the expansion's end, as
/// bash saves and restores its parser state around a command substitution. The stack is the
/// recursion itself: each enclosing frame is held by the call consuming the expansion nested
/// in it.
#[derive(Clone, Debug, Default)]
struct ExpansionFrame {
    /// The expansion whose contents this frame is, if any (none at the bottom).
    expansion: Option<NestedExpansion>,
    /// How many frames enclose this one; see [`MAX_EXPANSION_NESTING`].
    depth: u32,
    /// Current state of parsing here-documents.
    here_state: HereState,
    /// Ordered queue of here tags for which we're still looking for matching here-document bodies.
    current_here_tags: Vec<HereTag>,
    /// How many closing chars (see [`NestedExpansion::closing_char`]) the expansion needs
    /// before it ends. Its contents may use the closing char too, paired with an opener of
    /// their own (see [`NestedExpansion::opening_char`]); each opener read needs one more.
    closers_needed: usize,
    /// How many parens of the `((...))` command being read are still open, if any (zero
    /// outside one). Within one, `<<` is a shift rather than a here-document operator. (In an
    /// arithmetic expansion's contents, `<<` is always a shift; see
    /// [`NestedExpansion::is_arithmetic`].)
    arithmetic_command_parens: usize,
}

impl ExpansionFrame {
    /// Notes that `c` was read unquoted from the frame's own contents (not from anything
    /// nested in them), keeping count of the closing chars the expansion needs: an opener
    /// needs one more, and a closing char that doesn't end the expansion closes an opener.
    fn note_unquoted_char(&mut self, c: char) {
        let Some(expansion) = self.expansion else {
            return;
        };
        if expansion.opening_char() == Some(c) {
            self.closers_needed += 1;
        } else if expansion.closing_char() == c {
            // A closing char that ends the expansion is taken as such before it's read as
            // contents, so it never gets here.
            debug_assert!(self.closers_needed > 1, "read the expansion's closing char");
            self.closers_needed -= 1;
        }
    }
}

/// Options controlling how the tokenizer operates.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct TokenizerOptions {
    /// Whether or not to enable extended globbing patterns (extglob).
    pub enable_extended_globbing: bool,
    /// Whether or not to operate in POSIX compliance mode.
    pub posix_mode: bool,
    /// Whether or not we're running in SH emulation mode.
    pub sh_mode: bool,
}

impl Default for TokenizerOptions {
    fn default() -> Self {
        Self {
            enable_extended_globbing: true,
            posix_mode: false,
            sh_mode: false,
        }
    }
}

/// Maximum depth of nested expansions (see [`NestedExpansion`]) the tokenizer will descend
/// into, so deeply nested input can't overflow the stack while it's tokenized. Real scripts
/// rarely nest more than a few levels: of ~2,900 surveyed (including bash-completion and
/// ble.sh), none nested more than 4.
///
/// This bounds only the tokenizer's own recursion. Words keep their nesting, so later passes
/// over them (e.g., expanding them) recurse as deeply, with stack frames of their own that may
/// be far larger (in an unoptimized build, over 100 KiB per level of `${...}` expansion).
pub(crate) const MAX_EXPANSION_NESTING: u32 = 32;

/// A tokenizer for shell scripts.
pub(crate) struct Tokenizer<'a, R: ?Sized + std::io::BufRead> {
    char_reader: std::iter::Peekable<utf8_chars::Chars<'a, R>>,
    /// Chars read ahead and put back, to be read again before any further input; see
    /// [`Tokenizer::read_here_doc_bodies_left_pending`].
    unread: Option<UnreadChars>,
    /// Whether to read the bodies of here-documents left pending when a nested expansion ends;
    /// see [`Tokenizer::read_here_doc_bodies_left_pending`]. [`command_substitution_body`]
    /// doesn't: it wants only the text before the `)`, and the bodies follow it.
    read_bodies_left_pending: bool,
    cross_state: CrossTokenParseState,
    options: TokenizerOptions,
}

/// Chars read ahead of the tokenizer and put back.
struct UnreadChars {
    /// The chars, in reverse order (the next to be read last).
    chars: Vec<char>,
    /// Where the cursor resumes once they've all been read again: where it was after reading
    /// ahead, not where it would be after the chars themselves.
    resume_at: SourcePosition,
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
        Self {
            start_position: start_position.to_owned(),
            token_so_far: String::new(),
            token_is_operator: false,
            in_escape: false,
            quote_mode: QuoteMode::None,
        }
    }

    pub fn pop(&mut self, end_position: &SourcePosition) -> Token {
        let end = Arc::new(end_position.to_owned());
        let token_location = SourceSpan {
            start: Arc::new(std::mem::take(&mut self.start_position)),
            end,
        };

        let token = if std::mem::take(&mut self.token_is_operator) {
            Token::Operator(std::mem::take(&mut self.token_so_far), token_location)
        } else {
            Token::Word(std::mem::take(&mut self.token_so_far), token_location)
        };

        end_position.clone_into(&mut self.start_position);
        self.in_escape = false;
        self.quote_mode = QuoteMode::None;

        token
    }

    pub const fn started_token(&self) -> bool {
        !self.token_so_far.is_empty()
    }

    /// Returns true if the token so far consists only of blanks.
    ///
    /// This can only happen when tokenizing with `include_space`, where blanks are accumulated
    /// into the token so that the original text of a nested expansion can be reproduced. Such
    /// blanks are not a word, so a `#` following them still begins a comment.
    pub fn only_blanks_so_far(&self) -> bool {
        !self.token_so_far.is_empty() && self.token_so_far.chars().all(is_blank)
    }

    pub fn append_char(&mut self, c: char) {
        self.token_so_far.push(c);
    }

    pub fn append_str(&mut self, s: &str) {
        self.token_so_far.push_str(s);
    }

    pub const fn unquoted(&self) -> bool {
        !self.in_escape && matches!(self.quote_mode, QuoteMode::None)
    }

    pub fn current_token(&self) -> &str {
        &self.token_so_far
    }

    pub fn is_specific_operator(&self, operator: &str) -> bool {
        self.token_is_operator && self.current_token() == operator
    }

    pub const fn in_operator(&self) -> bool {
        self.token_is_operator
    }

    fn is_newline(&self) -> bool {
        self.token_so_far == "\n"
    }

    fn replace_with_here_doc(&mut self, s: String) {
        self.token_so_far = s;
    }

    #[allow(clippy::too_many_lines)]
    pub fn delimit_current_token(
        &mut self,
        reason: TokenEndReason,
        cross_token_state: &mut CrossTokenParseState,
    ) -> Result<Option<TokenizeResult>, TokenizerError> {
        // If we don't have anything in the token, then don't yield an empty string token
        // *unless* it's the body of a here document.
        if !self.started_token() && !matches!(reason, TokenEndReason::HereDocumentBodyEnd) {
            return Ok(Some(TokenizeResult {
                reason,
                token: None,
            }));
        }

        let current_here_state = std::mem::take(&mut cross_token_state.frame.here_state);
        match current_here_state {
            HereState::NextTokenIsHereTag { remove_tabs } => {
                // Don't yield the operator as a token yet. We need to make sure we collect
                // up everything we need for all the here-documents with tags on this line.
                let operator_token_result = TokenizeResult {
                    reason,
                    token: Some(self.pop(&cross_token_state.cursor)),
                };

                cross_token_state.frame.here_state = HereState::CurrentTokenIsHereTag {
                    remove_tabs,
                    operator_token_result,
                };

                return Ok(None);
            }
            HereState::CurrentTokenIsHereTag {
                remove_tabs,
                operator_token_result,
            } => {
                // A here tag must be a word; an operator in its place (e.g., a newline, `;`
                // or `)`) means it's missing.
                if self.token_is_operator {
                    return Err(TokenizerError::MissingHereTag(
                        self.current_token().to_owned(),
                    ));
                }

                // Inside a nested expansion, where blanks are kept, blanks between the
                // operator and the tag may be delimited as a token of their own (e.g., the
                // first of the two blanks in `$(cat <<  EOF)`). They're not the tag; skip
                // them and keep waiting for it.
                if self.only_blanks_so_far() {
                    self.pop(&cross_token_state.cursor);
                    cross_token_state.frame.here_state = HereState::CurrentTokenIsHereTag {
                        remove_tabs,
                        operator_token_result,
                    };
                    return Ok(None);
                }

                cross_token_state.frame.here_state = HereState::NextLineIsHereDoc;

                // Include the trailing \n in the here tag so it's easier to check against. Kept
                // blanks may lead the token (e.g., `$(cat << EOF)`); strip only those, since
                // anything else (even a carriage return) is part of the tag.
                let tag = std::format!("{}\n", self.current_token().trim_start_matches(is_blank));
                let tag_was_escaped_or_quoted = tag.contains(is_quoting_char);

                let tag_token_result = TokenizeResult {
                    reason,
                    token: Some(self.pop(&cross_token_state.cursor)),
                };

                cross_token_state.frame.current_here_tags.push(HereTag {
                    tag,
                    tag_was_escaped_or_quoted,
                    remove_tabs,
                    position: cross_token_state.cursor.clone(),
                    tokens: vec![operator_token_result, tag_token_result],
                    pending_tokens_after: vec![],
                });

                return Ok(None);
            }
            HereState::NextLineIsHereDoc => {
                if self.is_newline() {
                    cross_token_state.frame.here_state = HereState::InHereDocs;
                } else {
                    cross_token_state.frame.here_state = HereState::NextLineIsHereDoc;
                }

                if let Some(last_here_tag) = cross_token_state.frame.current_here_tags.last_mut() {
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
                let completed_here_tag = cross_token_state.frame.current_here_tags.remove(0);

                // First queue the redirection operator and (start) here-tag.
                cross_token_state
                    .queued_tokens
                    .extend(completed_here_tag.tokens);

                // Leave a hint that we are about to start a here-document.
                cross_token_state.queued_tokens.push(TokenizeResult {
                    reason: TokenEndReason::HereDocumentBodyStart,
                    token: None,
                });

                // Then queue the body document we just finished.
                cross_token_state.queued_tokens.push(TokenizeResult {
                    reason,
                    token: Some(self.pop(&cross_token_state.cursor)),
                });

                // Then queue up the (end) here-tag.
                let end_tag = if completed_here_tag.tag_was_escaped_or_quoted {
                    unquote_str(&completed_here_tag.tag)
                } else {
                    completed_here_tag.tag
                };
                self.append_str(end_tag.trim_end_matches('\n'));
                cross_token_state.queued_tokens.push(TokenizeResult {
                    reason: TokenEndReason::HereDocumentEndTag,
                    token: Some(self.pop(&cross_token_state.cursor)),
                });

                // Now we're ready to queue up any tokens that came between the completed
                // here tag and the next here tag (or newline after it if it was the last).
                cross_token_state
                    .queued_tokens
                    .extend(completed_here_tag.pending_tokens_after);

                if cross_token_state.frame.current_here_tags.is_empty() {
                    cross_token_state.frame.here_state = HereState::None;
                } else {
                    cross_token_state.frame.here_state = HereState::InHereDocs;
                }

                return Ok(None);
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
    tokenize_str_with_options(input, &TokenizerOptions::default())
}

/// Break the given input shell script string into tokens, returning the tokens.
///
/// # Arguments
///
/// * `input` - The shell script to tokenize.
/// * `options` - Options controlling how the tokenizer operates.
pub fn tokenize_str_with_options(
    input: &str,
    options: &TokenizerOptions,
) -> Result<Vec<Token>, TokenizerError> {
    uncached_tokenize_string(input, options)
}

#[cached::macros::cached(
    name = "TOKENIZE_CACHE",
    max_size = 64,
    key = "(String, TokenizerOptions)",
    convert = r#"{ (input.to_owned(), options.to_owned()) }"#
)]
fn uncached_tokenize_string(
    input: &str,
    options: &TokenizerOptions,
) -> Result<Vec<Token>, TokenizerError> {
    uncached_tokenize_str(input, options)
}

/// Break the given input shell script string into tokens, returning the tokens.
/// No caching is performed.
///
/// # Arguments
///
/// * `input` - The shell script to tokenize.
pub fn uncached_tokenize_str(
    input: &str,
    options: &TokenizerOptions,
) -> Result<Vec<Token>, TokenizerError> {
    let mut reader = std::io::BufReader::new(input.as_bytes());
    let mut tokenizer = crate::tokenizer::Tokenizer::new(&mut reader, options);

    let mut tokens = vec![];
    loop {
        match tokenizer.next_token()? {
            TokenizeResult {
                token: Some(token), ..
            } => tokens.push(token),
            TokenizeResult {
                reason: TokenEndReason::EndOfInput,
                ..
            } => break,
            _ => (),
        }
    }

    Ok(tokens)
}

/// Given the text following the `$(` that opens a command substitution, returns the
/// command's text up to (but not including) the `)` that closes it. The command is
/// tokenized to find that `)`, so quoting, nested expansions, and here-document bodies
/// are skipped over exactly as they are when tokenizing a full script.
///
/// # Errors
///
/// Returns an error if the closing `)` can't be found: either the input ends first
/// ([`TokenizerError::UnterminatedExpansion`]), or tokenizing the command fails before
/// reaching it (e.g., an unterminated quote or here-document). This is not a syntax
/// check of the command itself; it's parsed separately, when it is executed.
pub(crate) fn command_substitution_body<'a>(
    input: &'a str,
    options: &TokenizerOptions,
) -> Result<&'a str, TokenizerError> {
    expansion_body(input, options, NestedExpansion::CommandSubstitution)
}

/// Given the text following `$[`, returns the arithmetic expression before its closing `]`.
/// The tokenizer counts nested brackets without recursing and skips quoted brackets and
/// nested expansions. Arithmetic syntax is checked separately when the expression is evaluated.
pub(crate) fn legacy_arithmetic_expansion_body<'a>(
    input: &'a str,
    options: &TokenizerOptions,
) -> Result<&'a str, TokenizerError> {
    expansion_body(input, options, NestedExpansion::LegacyArithmeticExpansion)
}

fn expansion_body<'a>(
    input: &'a str,
    options: &TokenizerOptions,
    expansion: NestedExpansion,
) -> Result<&'a str, TokenizerError> {
    let mut reader = input.as_bytes();
    let mut tokenizer = Tokenizer::new(&mut reader, options);
    tokenizer.read_bodies_left_pending = false;

    // Consume tokens through the expansion's closing character. We don't need the token
    // text collected in `state`: it's a normalized rendering, not a slice of `input`.
    let mut state = TokenParseState::new(&tokenizer.cross_state.cursor);
    tokenizer.consume_nested_expansion(&mut state, expansion)?;

    // The cursor counts characters (not bytes) consumed, the last being the closing character;
    // convert the characters before it to a byte length.
    let body_len = input
        .chars()
        .take(tokenizer.cross_state.cursor.index - 1)
        .map(char::len_utf8)
        .sum();
    Ok(input.split_at(body_len).0)
}

impl<'a, R: ?Sized + std::io::BufRead> Tokenizer<'a, R> {
    pub fn new(reader: &'a mut R, options: &TokenizerOptions) -> Self {
        Tokenizer {
            options: options.clone(),
            char_reader: reader.chars().peekable(),
            unread: None,
            read_bodies_left_pending: true,
            cross_state: CrossTokenParseState {
                cursor: SourcePosition {
                    index: 0,
                    line: 1,
                    column: 1,
                },
                queued_tokens: vec![],
                frame: ExpansionFrame::default(),
            },
        }
    }

    #[expect(clippy::unnecessary_wraps)]
    pub fn current_location(&self) -> Option<SourcePosition> {
        Some(self.cross_state.cursor.clone())
    }

    fn next_char(&mut self) -> Result<Option<char>, TokenizerError> {
        let c = match self.unread.as_mut().and_then(|unread| unread.chars.pop()) {
            Some(c) => Some(c),
            None => self
                .char_reader
                .next()
                .transpose()
                .map_err(TokenizerError::ReadError)?,
        };

        if let Some(ch) = c {
            if ch == '\n' {
                self.cross_state.cursor.line += 1;
                self.cross_state.cursor.column = 1;
            } else {
                self.cross_state.cursor.column += 1;
            }
            self.cross_state.cursor.index += 1;
        }

        if let Some(unread) = self.unread.take_if(|unread| unread.chars.is_empty()) {
            self.cross_state.cursor = unread.resume_at;
        }

        Ok(c)
    }

    fn consume_char(&mut self) -> Result<(), TokenizerError> {
        let _ = self.next_char()?;
        Ok(())
    }

    fn peek_char(&mut self) -> Result<Option<char>, TokenizerError> {
        if let Some(c) = self.unread.as_ref().and_then(|unread| unread.chars.last()) {
            return Ok(Some(*c));
        }

        match self.char_reader.peek() {
            Some(result) => match result {
                Ok(c) => Ok(Some(*c)),
                Err(_) => Err(TokenizerError::FailedDecoding),
            },
            None => Ok(None),
        }
    }

    /// Consumes the rest of a nested expansion, whose opening (e.g., `$(`) has already been
    /// appended to `state`'s token, appending the rest to it as well.
    fn consume_nested_expansion(
        &mut self,
        state: &mut TokenParseState,
        expansion: NestedExpansion,
    ) -> Result<(), TokenizerError> {
        // This function and `next_token` are mutually recursive, as deep as the *input* nests
        // expansions; bound it so pathological input can't overflow the stack.
        let depth = self.cross_state.frame.depth + 1;
        if depth > MAX_EXPANSION_NESTING {
            return Err(TokenizerError::ExpansionNestingTooDeep);
        }

        // Push a frame for the expansion's contents, where none of the enclosing frame's state
        // applies: they're all part of one word, so none of their tokens can be (or end) a
        // here tag pending before it, and they can't be in a `((...))` command. Pop it
        // (restoring the enclosing frame) once they end.
        let frame = ExpansionFrame {
            expansion: Some(expansion),
            depth,
            closers_needed: expansion.closers_after_opening(),
            ..ExpansionFrame::default()
        };

        // Frames share the queue of tokens, but none are queued across a push or pop: tokens
        // are only read from the input (and so expansions only consumed) once the queue is
        // empty, and consuming one leaves it empty.
        debug_assert!(self.cross_state.queued_tokens.is_empty());
        let enclosing = std::mem::replace(&mut self.cross_state.frame, frame);
        let result = self.consume_expansion_contents(state, expansion);
        debug_assert!(result.is_err() || self.cross_state.queued_tokens.is_empty());
        self.cross_state.frame = enclosing;
        result
    }

    /// Consumes the tokens of a nested expansion's contents, through its closing char,
    /// appending their text to `state`'s token.
    fn consume_expansion_contents(
        &mut self,
        state: &mut TokenParseState,
        expansion: NestedExpansion,
    ) -> Result<(), TokenizerError> {
        let mut bodies = String::new();

        loop {
            let result = self.next_token()?;

            match result.reason {
                // The bodies set aside go after the rest of the line holding their tags.
                TokenEndReason::UnescapedNewLine => state.append_str(&std::mem::take(&mut bodies)),
                TokenEndReason::ExpansionClosingChar => {
                    // The closing char comes next; consume it too.
                    let closing_char = self
                        .next_char()?
                        .ok_or_else(|| expansion.unterminated_error())?;

                    let here_state = &self.cross_state.frame.here_state;
                    if here_state.awaiting_tag() {
                        // The expansion ended right after a here-document operator, as in
                        // `$(cat <<)`.
                        return Err(TokenizerError::MissingHereTag(closing_char.to_string()));
                    } else if matches!(here_state, HereState::NextLineIsHereDoc)
                        && self.read_bodies_left_pending
                    {
                        self.read_here_doc_bodies_left_pending(state)?;
                    }

                    state.append_char(closing_char);
                    return Ok(());
                }
                TokenEndReason::EndOfInput => return Err(expansion.unterminated_error()),
                _ => (),
            }

            append_expansion_token(state, &mut bodies, &result);
        }
    }

    /// Reads the bodies of here-documents still pending when a nested expansion's contents
    /// end, as in `$(cat <<EOF)`, completing the expansion's text: appends to `state`'s token
    /// the tokens held back for them, then the bodies, as though the line ended just before
    /// the expansion's closing char (consumed, but not yet appended).
    ///
    /// Bash accepts this (with a warning), reading the bodies from the lines after the current
    /// one, ahead of any other here-documents pending by then. So set the rest of the line
    /// aside, read the bodies, and put it back to be tokenized as though they weren't there.
    fn read_here_doc_bodies_left_pending(
        &mut self,
        state: &mut TokenParseState,
    ) -> Result<(), TokenizerError> {
        let rest_of_line_start = self.cross_state.cursor.clone();
        let mut rest_of_line = vec![];
        while let Some(c) = self.next_char()? {
            rest_of_line.push(c);
            if c == '\n' {
                break;
            }
        }

        // As when a line ends, the held tokens come out in order, with the bodies (and end
        // tags) interleaved; the bodies go after all of them.
        self.cross_state.frame.here_state = HereState::InHereDocs;
        let mut bodies = String::new();
        while !matches!(self.cross_state.frame.here_state, HereState::None)
            || !self.cross_state.queued_tokens.is_empty()
        {
            let result = self.next_token()?;
            append_expansion_token(state, &mut bodies, &result);
        }
        state.append_str(&bodies);
        state.append_char('\n');

        if !rest_of_line.is_empty() {
            rest_of_line.reverse();
            let resume_at = std::mem::replace(&mut self.cross_state.cursor, rest_of_line_start);
            self.unread = Some(UnreadChars {
                chars: rest_of_line,
                resume_at,
            });
        }

        Ok(())
    }

    /// Consumes a `$` or backquote (`c`, the next char) and whatever expansion it begins
    /// (e.g., `$(...)`, `${...}`, `` `...` ``), appending all of it to `state`'s token.
    /// `in_double_quotes` says whether `c` is inside double quotes, which `state` doesn't
    /// always track (e.g., within an extglob pattern).
    #[allow(clippy::unwrap_in_result)]
    fn consume_dollar_or_backquote(
        &mut self,
        state: &mut TokenParseState,
        c: char,
        in_double_quotes: bool,
    ) -> Result<(), TokenizerError> {
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
                    let expansion = if matches!(self.peek_char()?, Some('(')) {
                        // Consume the second '(' and add it to the token.
                        state.append_char(self.next_char()?.unwrap());
                        NestedExpansion::ArithmeticExpansion
                    } else {
                        NestedExpansion::CommandSubstitution
                    };

                    self.consume_nested_expansion(state, expansion)?;
                }

                Some('[') => {
                    // Add the '$' we already consumed to the token.
                    state.append_char('$');

                    // Consume the '[' and add it to the token.
                    state.append_char(self.next_char()?.unwrap());

                    self.consume_nested_expansion(
                        state,
                        NestedExpansion::LegacyArithmeticExpansion,
                    )?;
                }

                Some('{') => {
                    // Add the '$' we already consumed to the token.
                    state.append_char('$');

                    // Consume the '{' and add it to the token.
                    state.append_char(self.next_char()?.unwrap());

                    self.consume_nested_expansion(state, NestedExpansion::ParameterExpansion)?;
                }
                // `$$` is a parameter of its own; whatever follows it starts nothing.
                Some('$') => {
                    self.consume_char()?;
                    state.append_str("$$");
                }

                // `$'...'` is ANSI-C quoting, but only outside double quotes.
                Some('\'') if !in_double_quotes => {
                    state.append_char('$');
                    state.quote_mode = QuoteMode::AnsiC(self.cross_state.cursor.clone());
                    state.append_char(self.next_char()?.unwrap());
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

        Ok(())
    }

    /// Returns the next token from the input stream.
    ///
    /// Within a nested expansion's contents, the closing char that ends the expansion ends the
    /// token in progress and is then reported on its own (see
    /// [`TokenEndReason::ExpansionClosingChar`]), and blanks are kept in tokens so that the
    /// expansion's original text can be reproduced.
    #[expect(clippy::cognitive_complexity)]
    #[expect(clippy::if_same_then_else)]
    #[expect(clippy::panic_in_result_fn)]
    #[expect(clippy::too_many_lines)]
    #[allow(clippy::unwrap_in_result)]
    pub fn next_token(&mut self) -> Result<TokenizeResult, TokenizerError> {
        let mut state = TokenParseState::new(&self.cross_state.cursor);
        let mut result: Option<TokenizeResult> = None;

        let expansion = self.cross_state.frame.expansion;
        let closing_char = expansion.map(NestedExpansion::closing_char);
        let include_space = expansion.is_some();
        // Directly inside `${...}`, text is a word, not a command: bash recognizes no
        // here-documents, comments or extglobs there.
        let in_parameter_expansion = expansion == Some(NestedExpansion::ParameterExpansion);
        let in_arithmetic_expansion = expansion.is_some_and(NestedExpansion::is_arithmetic);

        while result.is_none() {
            // First satisfy token results from our queue. Once we exhaust the queue then
            // we'll look at the input stream.
            if !self.cross_state.queued_tokens.is_empty() {
                return Ok(self.cross_state.queued_tokens.remove(0));
            }

            let next = self.peek_char()?;
            let c = next.unwrap_or('\0');

            // When we hit the end of the input, then we're done with the current token (if there is
            // one).
            if next.is_none() {
                // TODO(tokenizer): Verify we're not waiting on some terminating character?
                // Verify we're out of all quotes.
                if state.in_escape {
                    return Err(TokenizerError::UnterminatedEscapeSequence);
                }
                match state.quote_mode {
                    QuoteMode::None => (),
                    QuoteMode::AnsiC(pos) => {
                        return Err(TokenizerError::UnterminatedAnsiCQuote(pos));
                    }
                    QuoteMode::Single(pos) => {
                        return Err(TokenizerError::UnterminatedSingleQuote(pos));
                    }
                    QuoteMode::Double(pos) => {
                        return Err(TokenizerError::UnterminatedDoubleQuote(pos));
                    }
                }

                // Verify we're not in a here document.
                if !matches!(self.cross_state.frame.here_state, HereState::None) {
                    // If the input ends while a here tag is expected, finish the tag in
                    // progress so it's recorded (and reported below); without one, the tag
                    // is missing.
                    if self.cross_state.frame.here_state.awaiting_tag() && state.started_token() {
                        state.delimit_current_token(
                            TokenEndReason::EndOfInput,
                            &mut self.cross_state,
                        )?;
                    }
                    if self.cross_state.frame.here_state.awaiting_tag() {
                        return Err(TokenizerError::MissingHereTagAtEndOfInput);
                    }

                    // Only a body can end at an end tag. Outside one, an empty tag would
                    // "match" on every pass without making progress, looping forever.
                    if matches!(self.cross_state.frame.here_state, HereState::InHereDocs)
                        && self.remove_here_end_tag(&mut state, &mut result, false)?
                    {
                        // If we hit end tag without a trailing newline, try to get next token.
                        continue;
                    }

                    let tag_names = self
                        .cross_state
                        .frame
                        .current_here_tags
                        .iter()
                        .map(|tag| std::format!("`{}`", tag.tag.trim_end_matches('\n')))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let tag_positions = self
                        .cross_state
                        .frame
                        .current_here_tags
                        .iter()
                        .map(|tag| std::format!("{}", tag.position))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(TokenizerError::UnterminatedHereDocuments(
                        tag_names,
                        tag_positions,
                    ));
                }

                result = state
                    .delimit_current_token(TokenEndReason::EndOfInput, &mut self.cross_state)?;
            //
            // Handle being in a here document.
            //
            } else if matches!(self.cross_state.frame.here_state, HereState::InHereDocs) {
                //
                // For now, just include the character in the current token. We also check
                // if there are leading tabs to be removed.
                //
                if !self.cross_state.frame.current_here_tags.is_empty()
                    && self.cross_state.frame.current_here_tags[0].remove_tabs
                    && (!state.started_token() || state.current_token().ends_with('\n'))
                    && c == '\t'
                {
                    // Consume it but don't include it.
                    self.consume_char()?;
                } else {
                    self.consume_char()?;
                    state.append_char(c);

                    // See if this was a newline character following the terminating here tag.
                    if c == '\n' {
                        self.remove_here_end_tag(&mut state, &mut result, true)?;
                    }
                }
            //
            // Look for the closing char that ends the nested expansion being consumed (one that
            // closes an opener in its contents is read like any other char, below). An operator
            // in progress is delimited first, below, so that it takes effect: a newline may start
            // a here-doc body, and a `<<` expects its tag (and reports it missing). Any other
            // token in progress is delimited here, leaving the closing char to be reported on
            // its own next time around.
            //
            } else if state.unquoted()
                && !state.in_operator()
                && closing_char == Some(c)
                && self.cross_state.frame.closers_needed == 1
            {
                let reason = if state.started_token() {
                    TokenEndReason::Other
                } else {
                    TokenEndReason::ExpansionClosingChar
                };
                result = state.delimit_current_token(reason, &mut self.cross_state)?;
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
                    // In an arithmetic context, don't consider << and <<- special. They're
                    // not here-docs, they're either a left-shift operator or a left-shift
                    // operator followed by a unary minus operator.
                    //
                    if in_arithmetic_expansion || in_parameter_expansion {
                        // Operators have no effect here: arithmetic contents are arithmetic
                        // throughout, and `${...}` contents are a word, not a command.
                    } else if self.cross_state.frame.arithmetic_command_parens > 0 {
                        // A `((...))` command ends at the `)` that balances its first `(`.
                        let parens = &mut self.cross_state.frame.arithmetic_command_parens;
                        if state.is_specific_operator("(") {
                            *parens += 1;
                        } else if state.is_specific_operator(")") {
                            *parens -= 1;
                        }
                    } else if state.is_specific_operator("<<") {
                        self.cross_state.frame.here_state =
                            HereState::NextTokenIsHereTag { remove_tabs: false };
                    } else if state.is_specific_operator("<<-") {
                        self.cross_state.frame.here_state =
                            HereState::NextTokenIsHereTag { remove_tabs: true };
                    } else if state.is_specific_operator("(") && c == '(' {
                        // A `((...))` command starts with this `(`.
                        self.cross_state.frame.arithmetic_command_parens = 1;
                    }

                    let reason = if state.current_token() == "\n" {
                        TokenEndReason::UnescapedNewLine
                    } else {
                        TokenEndReason::OperatorEnd
                    };

                    result = state.delimit_current_token(reason, &mut self.cross_state)?;
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
                    // (A `$'` was already taken as ANSI-C quoting where its `$` was read.)
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
            // Handle end of single-quote, double-quote, or ANSI-C quote.
            else if !state.in_escape
                && matches!(
                    state.quote_mode,
                    QuoteMode::Single(..) | QuoteMode::AnsiC(..)
                )
                && c == '\''
            {
                state.quote_mode = QuoteMode::None;
                self.consume_char()?;
                state.append_char(c);
            } else if !state.in_escape
                && matches!(state.quote_mode, QuoteMode::Double(..))
                && c == '\"'
            {
                state.quote_mode = QuoteMode::None;
                self.consume_char()?;
                state.append_char(c);
            }
            //
            // Handle end of escape sequence.
            // TODO(tokenizer): Handle double-quote specific escape sequences.
            else if state.in_escape {
                state.in_escape = false;
                self.consume_char()?;
                state.append_char(c);
            } else if (state.unquoted()
                || (matches!(state.quote_mode, QuoteMode::Double(_)) && !state.in_escape))
                && (c == '$' || c == '`')
            {
                // TODO(tokenizer): handle quoted $ or ` in a double quote
                let in_double_quotes = matches!(state.quote_mode, QuoteMode::Double(_));
                self.consume_dollar_or_backquote(&mut state, c, in_double_quotes)?;
            }
            //
            // [Extension]
            // If extended globbing is enabled, the last consumed character is an
            // unquoted start of an extglob pattern, *and* if the current character
            // is an open parenthesis, then this begins an extglob pattern.
            else if c == '('
                && self.options.enable_extended_globbing
                && !in_parameter_expansion
                && state.unquoted()
                && !state.in_operator()
                && state
                    .current_token()
                    .ends_with(|x| Self::can_start_extglob(x))
            {
                // Consume the '(' and append it.
                self.consume_char()?;
                state.append_char(c);

                let mut paren_depth = 1;
                // The char that closes the current quote, and whether backslash
                // escapes within it (it doesn't in '...', but does in "..." and $'...').
                let mut quote: Option<(char, bool)> = None;
                let mut after_dollar = false;

                // Keep consuming until we see the matching end ')'. Parens inside
                // quotes are literal pattern characters, not delimiters. As in bash,
                // `$(...)` and `${...}` are only nested constructs inside double
                // quotes; elsewhere their parens simply count toward the nesting.
                while paren_depth > 0 {
                    let Some(extglob_char) = self.peek_char()? else {
                        return Err(TokenizerError::UnterminatedExtendedGlob(
                            self.cross_state.cursor.clone(),
                        ));
                    };
                    let was_after_dollar = std::mem::take(&mut after_dollar);

                    let starts_nested_construct = match quote {
                        None => extglob_char == '`',
                        Some(('"', _)) => matches!(extglob_char, '`' | '$'),
                        Some(_) => false,
                    };
                    if starts_nested_construct {
                        // A `$` only gets here inside double quotes, so it's never taken as
                        // starting `$'...'`, which would set `state`'s quote mode rather than
                        // this loop's own `quote`.
                        let in_double_quotes = quote.is_some();
                        self.consume_dollar_or_backquote(
                            &mut state,
                            extglob_char,
                            in_double_quotes,
                        )?;
                        continue;
                    }

                    // Include it in the token.
                    self.consume_char()?;
                    state.append_char(extglob_char);

                    match extglob_char {
                        // Take the escaped char as-is. If the input ends instead, the next
                        // iteration reports the unterminated extglob.
                        '\\' if quote.is_none_or(|(_, escapes)| escapes) => {
                            if let Some(escaped_char) = self.next_char()? {
                                state.append_char(escaped_char);
                            }
                        }
                        c if quote.is_some_and(|(close, _)| close == c) => quote = None,
                        _ if quote.is_some() => (),
                        '\'' => quote = Some(('\'', was_after_dollar)),
                        '"' => quote = Some(('"', true)),
                        // `$$` is a parameter, not a `$` that could start `$'`.
                        '$' => after_dollar = !was_after_dollar,
                        '(' => paren_depth += 1,
                        ')' => paren_depth -= 1,
                        _ => (),
                    }
                }
            //
            // If the character *can* start an operator, then it will.
            //
            } else if state.unquoted() && Self::can_start_operator(c) {
                if state.started_token() {
                    result = state.delimit_current_token(
                        TokenEndReason::OperatorStart,
                        &mut self.cross_state,
                    )?;
                } else {
                    state.token_is_operator = true;
                    self.consume_char()?;
                    state.append_char(c);
                    self.cross_state.frame.note_unquoted_char(c);
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

                    // Where blanks are kept, this one begins the next token, as written.
                    if include_space {
                        continue;
                    }
                } else if include_space {
                    state.append_char(c);
                } else {
                    // Make sure we don't include this char in the token range.
                    state.start_position.column += 1;
                    state.start_position.index += 1;
                }

                self.consume_char()?;
            }
            //
            // N.B. Inside a parameter expansion, everything left belongs to the word, even
            // where no token was started: a `#` there never starts a comment (e.g., `${#x}`).
            //
            // Elsewhere, the `!only_blanks_so_far` clause keeps a comment recognizable inside a
            // nested expansion. Blanks are kept in tokens there, so a `#` after them follows a
            // token in progress; being only blanks, that token isn't a word, and the `#` still
            // starts a comment (e.g., in `$( #'<newline>)`).
            //
            else if !state.token_is_operator
                && (in_parameter_expansion
                    || (state.started_token() && !(c == '#' && state.only_blanks_so_far())))
            {
                self.consume_char()?;
                state.append_char(c);
                if state.unquoted() {
                    self.cross_state.frame.note_unquoted_char(c);
                }
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
            } else if state.started_token() {
                // In all other cases where we have an in-progress token, we delimit here.
                result =
                    state.delimit_current_token(TokenEndReason::Other, &mut self.cross_state)?;
            } else {
                // If we got here, then we don't have a token in progress and we're not starting an
                // operator. Add the character to a new token.
                self.consume_char()?;
                state.append_char(c);
                self.cross_state.frame.note_unquoted_char(c);
            }
        }

        let result = result.unwrap();

        Ok(result)
    }

    fn remove_here_end_tag(
        &mut self,
        state: &mut TokenParseState,
        result: &mut Option<TokenizeResult>,
        ends_with_newline: bool,
    ) -> Result<bool, TokenizerError> {
        // Bail immediately if we don't even have a *starting* here tag.
        if self.cross_state.frame.current_here_tags.is_empty() {
            return Ok(false);
        }

        let next_here_tag = &self.cross_state.frame.current_here_tags[0];

        let tag_str: Cow<'_, str> = if next_here_tag.tag_was_escaped_or_quoted {
            unquote_str(next_here_tag.tag.as_str()).into()
        } else {
            next_here_tag.tag.as_str().into()
        };

        let tag_str = if !ends_with_newline {
            tag_str
                .strip_suffix('\n')
                .unwrap_or_else(|| tag_str.as_ref())
        } else {
            tag_str.as_ref()
        };

        // At the end of input, the tag's line may lack its newline (`cat <<EOF⏎body⏎EOF`). An
        // empty tag would then match the nothing after the last newline, which isn't a line: as
        // in bash, its body only ends at an empty line.
        if tag_str.is_empty() {
            return Ok(false);
        }

        if let Some(current_token_without_here_tag) = state.current_token().strip_suffix(tag_str) {
            // Make sure that was either the start of the here document, or there
            // was a newline between the preceding part
            // and the tag.
            if current_token_without_here_tag.is_empty()
                || current_token_without_here_tag.ends_with('\n')
            {
                state.replace_with_here_doc(current_token_without_here_tag.to_owned());

                // Delimit the end of the here-document body.
                *result = state.delimit_current_token(
                    TokenEndReason::HereDocumentBodyEnd,
                    &mut self.cross_state,
                )?;

                return Ok(true);
            }
        }
        Ok(false)
    }

    const fn can_start_extglob(c: char) -> bool {
        matches!(c, '@' | '!' | '?' | '+' | '*')
    }

    const fn can_start_operator(c: char) -> bool {
        matches!(c, '&' | '(' | ')' | ';' | '\n' | '|' | '<' | '>')
    }

    fn is_operator(&self, s: &str) -> bool {
        // Handle non-POSIX operators.
        if !self.options.sh_mode && matches!(s, "<<<" | "&>" | "&>>" | ";;&" | ";&" | "|&") {
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

impl<R: ?Sized + std::io::BufRead> Iterator for Tokenizer<'_, R> {
    type Item = Result<TokenizeResult, TokenizerError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_token() {
            #[expect(clippy::manual_map)]
            Ok(result) => match result.token {
                Some(_) => Some(Ok(result)),
                None => None,
            },
            Err(e) => Some(Err(e)),
        }
    }
}

/// Appends the text of a token from a nested expansion's contents to the expansion's text in
/// `state`'s token, except for here-document bodies (and end tags): they come out ahead of the
/// rest of the line holding their tags, so they're set aside in `bodies`, to follow it.
fn append_expansion_token(
    state: &mut TokenParseState,
    bodies: &mut String,
    result: &TokenizeResult,
) {
    let text = result.token.as_ref().map_or("", Token::to_str);
    match result.reason {
        TokenEndReason::HereDocumentBodyStart => bodies.push('\n'),
        TokenEndReason::HereDocumentBodyEnd | TokenEndReason::HereDocumentEndTag => {
            bodies.push_str(text);
        }
        _ => state.append_str(text),
    }
}

const fn is_blank(c: char) -> bool {
    c == ' ' || c == '\t'
}

const fn does_char_newly_affect_quoting(state: &TokenParseState, c: char) -> bool {
    // If we're currently escaped, then nothing affects quoting.
    if state.in_escape {
        return false;
    }

    match state.quote_mode {
        // When we're in a double quote or ANSI-C quote, only a subset of escape
        // sequences are recognized.
        QuoteMode::Double(_) | QuoteMode::AnsiC(_) => {
            if c == '\\' {
                // TODO(tokenizer): handle backslash in double quote
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

const fn is_quoting_char(c: char) -> bool {
    matches!(c, '\\' | '\'' | '\"')
}

/// Return a string with all the quoting removed.
///
/// # Arguments
///
/// * `s` - The string to unquote.
pub fn unquote_str(s: &str) -> String {
    let mut result = String::new();

    let mut in_escape = false;
    for c in s.chars() {
        match c {
            c if in_escape => {
                result.push(c);
                in_escape = false;
            }
            '\\' => in_escape = true,
            c if is_quoting_char(c) => (),
            c => result.push(c),
        }
    }

    result
}

#[cfg(test)]
mod tests {

    use super::*;
    use anyhow::Result;
    use insta::assert_ron_snapshot;
    use pretty_assertions::{assert_eq, assert_matches};

    #[derive(serde::Serialize, serde::Deserialize)]
    struct TokenizerResult<'a> {
        input: &'a str,
        result: Vec<Token>,
    }

    fn test_tokenizer(input: &str) -> Result<TokenizerResult<'_>> {
        Ok(TokenizerResult {
            input,
            result: tokenize_str(input)?,
        })
    }

    #[test]
    fn tokenize_empty() -> Result<()> {
        let tokens = tokenize_str("")?;
        assert_eq!(tokens.len(), 0);
        Ok(())
    }

    #[test]
    fn tokenize_line_continuation() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"a\
bc"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_operators() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("a>>b")?);
        Ok(())
    }

    #[test]
    fn tokenize_comment() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"a #comment
"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_comment_in_command_substitution() {
        // A comment inside `$( )` is a comment however many blanks precede its `#`, even though
        // blanks are kept in tokens there, so the apostrophe in its text doesn't open a quote.
        // The comment is dropped from the text reconstructed for the substitution, leaving just
        // the blanks that preceded it, as written.
        for prefix in ["", " ", "  ", "   ", "\t", "\t\t", " \t", "\t "] {
            let input = format!("$({prefix}# it's a comment\n)\n");
            let tokens = tokenize_str(input.as_str()).unwrap();
            let token_strs: Vec<_> = tokens.iter().map(Token::to_str).collect();
            assert_eq!(
                token_strs,
                [format!("$({prefix}\n)").as_str(), "\n"],
                "tokenizing {input:?}"
            );
        }
    }

    #[test]
    fn tokenize_comment_at_eof() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r"a #comment")?);
        Ok(())
    }

    #[test]
    fn tokenize_empty_here_doc() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"cat <<HERE
HERE
"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_here_doc() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"cat <<HERE
SOMETHING
HERE
echo after
"
        )?);
        assert_ron_snapshot!(test_tokenizer(
            r"cat <<HERE
SOMETHING
HERE
"
        )?);
        assert_ron_snapshot!(test_tokenizer(
            r"cat <<HERE
SOMETHING
HERE

"
        )?);
        assert_ron_snapshot!(test_tokenizer(
            r"cat <<HERE
SOMETHING
HERE"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_here_doc_with_tab_removal() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"cat <<-HERE
	SOMETHING
	HERE
"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_here_doc_with_other_tokens() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"cat <<EOF | wc -l
A B C
1 2 3
D E F
EOF
"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_multiple_here_docs() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"cat <<HERE1 <<HERE2
SOMETHING
HERE1
OTHER
HERE2
echo after
"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_unterminated_here_doc() {
        let result = tokenize_str(
            r"cat <<HERE
SOMETHING
",
        );
        assert!(result.is_err());
    }

    #[test]
    fn tokenize_here_tag_with_unterminated_expansion() {
        // An expansion in a here tag, left open at the end of the input. What's inside it (often
        // just blanks here) is part of the tag's word, not a tag of its own, so the input simply
        // ends mid-expansion.
        for input in [
            "<<E$[\t\t",
            "<<-E$[\t\t",
            "<<E$(\t\t",
            "<<E$((\t\t",
            "<<E\"$[\t\t",
            "x <<$(( )",
            "cat <<'' $(",
        ] {
            assert_matches!(
                tokenize_str(input),
                Err(TokenizerError::UnterminatedExpansion),
                "for {input:?}"
            );
        }
    }

    #[test]
    fn tokenize_here_doc_with_empty_quoted_tag_at_end_of_input() {
        // Tags that are empty once unquoted, with the input ending before any body. An empty tag
        // matches anywhere, but only a body can end at it, and none has started.
        for input in ["cat <<'' ", "cat <<\"\" "] {
            assert_matches!(
                tokenize_str(input),
                Err(TokenizerError::UnterminatedHereDocuments(..)),
                "for {input:?}"
            );
        }
    }

    #[test]
    fn tokenize_here_tag_containing_expansions() {
        for tag in ["$(x)", "E${x}", "$((1 + 2))", "$[1]", "$(echo  a)", "\r"] {
            let input = std::format!("cat <<{tag}\nbody\n{tag}\n");
            assert_matches!(
                tokenize_str(input.as_str()),
                Ok(tokens) if tokens.iter().any(|t| matches!(t, Token::Word(w, _) if w == "body\n")),
                "for {input:?}"
            );
        }
    }

    #[test]
    fn tokenize_missing_here_tag_reports_what_was_found() {
        for (input, found) in [
            ("cat <<\n", "\n"),
            ("cat <<;", ";"),
            ("cat <<)|$(\n)", ")"),
            ("echo $(cat <<  )", ")"),
            ("echo $(cat <<)", ")"),
            ("echo \"$(cat <<-)\"", ")"),
        ] {
            assert_matches!(
                tokenize_str(input),
                Err(TokenizerError::MissingHereTag(actual)) if actual == found,
                "for {input:?}"
            );
        }
    }

    #[test]
    fn tokenize_input_ending_where_here_tag_expected_is_incomplete() {
        // More input could still supply the tag, as after a line continuation.
        for input in ["cat << ", "cat << \\\n", "cat <<\\\n"] {
            assert_matches!(
                tokenize_str(input),
                Err(e) if e.is_incomplete(),
                "for {input:?}"
            );
        }
    }

    #[test]
    fn tokenize_here_doc_operator_in_parameter_expansion_is_literal() {
        // Bash recognizes no operators directly inside `${...}`.
        for word in ["${x:-<<}", "${x:-<< foo}", "${x:-a<<b}", "${x:-<<-EOF}"] {
            let input = std::format!("echo {word}\n");
            assert_matches!(
                tokenize_str(input.as_str()),
                Ok(tokens) if tokens.iter().any(|t| matches!(t, Token::Word(w, _) if w == word)),
                "for {input:?}"
            );
        }
    }

    #[test]
    fn tokenize_words_verbatim() {
        for word in [
            // Extglob-like text inside `${...}` is just text.
            "${u:-a@(}",
            // `$$` is a parameter; neither a `{` nor a `'` after it starts anything.
            "$${u",
            "$$'a\\'x",
            // Nor does a `'` after an escaped `$`.
            "\\$'a\\'x",
            // Brackets nest in `$[...]`, as in array subscripts, but not those in quotes or
            // nested expansions.
            "$[ a[1] + 1 ]",
            "$[ a[ a[0] ] ]",
            "$[ ${a[1]} + 1 ]",
            "$[ $[1] + a[$[0]] ]",
            "$[ \"[\" ]",
            // A `$'` in double quotes starts nothing, even in an extglob.
            "@(\"$'x'\")",
            // Blanks inside `${...}` are kept as they are.
            "\"${x:-a\tb}\"",
        ] {
            assert_matches!(
                tokenize_str(word),
                Ok(tokens) if matches!(tokens.as_slice(), [Token::Word(w, _)] if w == word),
                "for {word:?}"
            );
        }
    }

    #[test]
    fn tokenize_here_doc_left_open_by_command_substitution() -> Result<()> {
        // The body is read from the lines after the substitution's, ahead of any other
        // here-documents pending by then, and becomes part of the substitution's text.
        for (input, expected) in [
            (
                "x=\"$(cat <<EOF)\"\nbody\nEOF\n",
                &["x=\"$(cat <<EOF\nbody\nEOF\n)\"", "\n"][..],
            ),
            (
                "echo \"[$(cat <<EOF | tr a-z A-Z)]\"\nbody\nEOF\n",
                &["echo", "\"[$(cat <<EOF | tr a-z A-Z\nbody\nEOF\n)]\"", "\n"],
            ),
            (
                "echo \"[$(cat <<A; cat <<-B)]\"\na\nA\n\tb\n\tB\n",
                &["echo", "\"[$(cat <<A; cat <<-B\na\nA\nb\nB\n)]\"", "\n"],
            ),
            (
                "echo \"[$(echo $(cat <<EOF))]\"\nbody\nEOF\n",
                &["echo", "\"[$(echo $(cat <<EOF\nbody\nEOF\n))]\"", "\n"],
            ),
            (
                "echo \"[${x:-$(cat <<EOF)}]\"\nbody\nEOF\n",
                &["echo", "\"[${x:-$(cat <<EOF\nbody\nEOF\n)}]\"", "\n"],
            ),
            (
                "cat <<A; echo \"[$(cat <<B)]\"\nb\nB\na\nA\n",
                &[
                    "cat",
                    "<<",
                    "A",
                    "a\n",
                    "A",
                    ";",
                    "echo",
                    "\"[$(cat <<B\nb\nB\n)]\"",
                    "\n",
                ],
            ),
            (
                "echo \"[$(cat <<A)]\" \"[$(cat <<B)]\"\na\nA\nb\nB\n",
                &[
                    "echo",
                    "\"[$(cat <<A\na\nA\n)]\"",
                    "\"[$(cat <<B\nb\nB\n)]\"",
                    "\n",
                ],
            ),
        ] {
            let tokens = tokenize_str(input)?;
            let token_strs: Vec<_> = tokens.iter().map(Token::to_str).collect();
            assert_eq!(token_strs, expected, "for {input:?}");
        }

        // The rest of the line keeps its place in the input, and the next line follows the body.
        let tokens = tokenize_str("echo \"$(cat <<EOF)\" x\nbody\nEOF\ny\n")?;
        let positions: Vec<_> = tokens
            .iter()
            .map(|t| {
                (
                    t.to_str(),
                    t.location().start.line,
                    t.location().start.column,
                )
            })
            .collect();
        assert_eq!(
            positions,
            [
                ("echo", 1, 1),
                ("\"$(cat <<EOF\nbody\nEOF\n)\"", 1, 6),
                ("x", 1, 21),
                ("\n", 1, 22),
                ("y", 4, 1),
                ("\n", 4, 2),
            ]
        );

        Ok(())
    }

    #[test]
    fn tokenize_unterminated_here_doc_names_its_tag() {
        assert_matches!(
            tokenize_str("cat <<]|$[\n]"),
            Err(TokenizerError::UnterminatedHereDocuments(tags, _)) if tags == "`]`"
        );
    }

    #[test]
    fn tokenize_deeply_nested_expansions_is_bounded() {
        // The input chooses how deeply expansions nest, and consuming them recurses, so the
        // nesting must be bounded rather than overflow the stack.
        for (open, close) in [("$(", ")"), ("\"$(", ")\""), ("${x:-", "}"), ("$[", "]")] {
            let input = std::format!("echo {}x{}", open.repeat(10_000), close.repeat(10_000));
            assert_matches!(
                tokenize_str(input.as_str()),
                Err(TokenizerError::ExpansionNestingTooDeep),
                "for {open:?}"
            );
        }
    }

    #[test]
    fn tokenize_nesting_up_to_limit() {
        let nested = |depth: u32| {
            let depth = depth as usize;
            tokenize_str(&std::format!(
                "echo {}x{}",
                "$(".repeat(depth),
                ")".repeat(depth)
            ))
        };
        assert!(nested(MAX_EXPANSION_NESTING).is_ok());
        assert_matches!(
            nested(MAX_EXPANSION_NESTING + 1),
            Err(TokenizerError::ExpansionNestingTooDeep)
        );
    }

    #[test]
    fn tokenize_missing_here_tag() {
        let result = tokenize_str(
            r"cat <<
",
        );
        assert!(result.is_err());
    }

    #[test]
    fn tokenize_here_doc_in_command_substitution() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"echo $(cat <<HERE
TEXT
HERE
)"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_here_doc_in_double_quoted_command_substitution() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r#"echo "$(cat <<HERE
TEXT
HERE
)""#
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_here_doc_in_double_quoted_command_substitution_with_space() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r#"echo "$(cat << HERE
TEXT
HERE
)""#
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_complex_here_docs_in_command_substitution() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(
            r"echo $(cat <<HERE1 <<HERE2 | wc -l
TEXT
HERE1
OTHER
HERE2
)"
        )?);
        Ok(())
    }

    #[test]
    fn tokenize_simple_backquote() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r"echo `echo hi`")?);
        Ok(())
    }

    #[test]
    fn tokenize_backquote_with_escape() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r"echo `echo\`hi`")?);
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
        // $( is consumed before the tokenizer knows whether it's $( or $((,
        // so it goes through consume_nested_expansion and yields UnterminatedExpansion.
        assert_matches!(
            tokenize_str("$("),
            Err(TokenizerError::UnterminatedExpansion)
        );
    }

    #[test]
    fn command_substitution_body_stops_at_closing_paren() -> Result<()> {
        let options = TokenizerOptions::default();
        assert_eq!(
            command_substitution_body("echo hi) rest", &options)?,
            "echo hi"
        );
        assert_eq!(
            command_substitution_body(r#"echo ")" (a)) rest"#, &options)?,
            r#"echo ")" (a)"#
        );
        assert_eq!(
            command_substitution_body("cat <<'EOF'\n\"it's ) `\nEOF\n) rest", &options)?,
            "cat <<'EOF'\n\"it's ) `\nEOF\n"
        );
        // A `)` right after the newline that starts a here-doc body belongs to the body.
        assert_eq!(
            command_substitution_body("cat <<E\n)\nE\n) rest", &options)?,
            "cat <<E\n)\nE\n"
        );
        assert_eq!(
            command_substitution_body("cat <<E\n)\nE\necho after) rest", &options)?,
            "cat <<E\n)\nE\necho after"
        );
        // A here-doc left open by the command (or by a substitution nested in it) doesn't
        // matter: only the text before the `)` is wanted, and any body would follow it.
        assert_eq!(
            command_substitution_body("cat <<EOF) rest\n", &options)?,
            "cat <<EOF"
        );
        assert_eq!(
            command_substitution_body("echo $(cat <<EOF)) rest\n", &options)?,
            "echo $(cat <<EOF)"
        );
        // A quoted `)` in an extglob closes neither the pattern nor the command.
        assert_eq!(
            command_substitution_body(r#"printf "%s" @(")")) rest"#, &options)?,
            r#"printf "%s" @(")")"#
        );
        // Multi-byte characters inside and after the command.
        assert_eq!(
            command_substitution_body("echo “é”) ü", &options)?,
            "echo “é”"
        );
        Ok(())
    }

    #[test]
    fn command_substitution_body_unterminated() {
        let options = TokenizerOptions::default();
        assert_matches!(
            command_substitution_body("echo hi", &options),
            Err(TokenizerError::UnterminatedExpansion)
        );
        assert_matches!(
            command_substitution_body("echo 'hi)", &options),
            Err(TokenizerError::UnterminatedSingleQuote(_))
        );
    }

    #[test]
    fn tokenize_unterminated_arithmetic_expansion() {
        assert_matches!(
            tokenize_str("$(("),
            Err(TokenizerError::UnterminatedExpansion)
        );
    }

    #[test]
    fn tokenize_unterminated_legacy_arithmetic_expansion() {
        assert_matches!(
            tokenize_str("$["),
            Err(TokenizerError::UnterminatedExpansion)
        );
    }

    #[test]
    fn tokenize_command_substitution() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("a$(echo hi)b c")?);
        Ok(())
    }

    #[test]
    fn tokenize_command_substitution_with_subshell() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("$( (:) )")?);
        Ok(())
    }

    #[test]
    fn tokenize_command_substitution_containing_extglob() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("echo $(echo !(x))")?);
        Ok(())
    }

    #[test]
    fn tokenize_extglob_with_quotes_and_escapes() -> Result<()> {
        for (input, expected) in [
            // Quoted parens don't close the pattern.
            (r#"@(")") y"#, [r#"@(")")"#, "y"]),
            (r"@(a|')'|b)x y", [r"@(a|')'|b)x", "y"]),
            (r#"@("(") y"#, [r#"@("(")"#, "y"]),
            // Escaped quote inside double quotes doesn't end the quoting.
            (r#"@("\")") y"#, [r#"@("\")")"#, "y"]),
            // Backslash is literal inside single quotes.
            (r"@('\')x y", [r"@('\')x", "y"]),
            // Escaped quote outside quotes doesn't start quoting.
            (r#"@(\") y"#, [r#"@(\")"#, "y"]),
            (r"@(\)) y", [r"@(\))", "y"]),
            // Backslash escapes a quote inside ANSI-C quotes, but not inside `\$'...'`.
            (r"@($'\'')x y", [r"@($'\'')x", "y"]),
            (r"@($'a\')'|b) y", [r"@($'a\')'|b)", "y"]),
            (r"@(\$'a\')x y", [r"@(\$'a\')x", "y"]),
            // `$$` is a parameter, so a quote after it is ANSI-C only if a third `$` follows.
            (r"@($$'a\')x y", [r"@($$'a\')x", "y"]),
            (r"@($$$'\'')x y", [r"@($$$'\'')x", "y"]),
            // Backquotes are a region of their own: quotes inside them don't nest.
            (r"@(}`'`)x y", [r"@(}`'`)x", "y"]),
            (r#"@(`echo \\"`)x y"#, [r#"@(`echo \\"`)x"#, "y"]),
            (r"@(`$'\'`)x y", [r"@(`$'\'`)x", "y"]),
            (r#"@(`echo ")"`)x y"#, [r#"@(`echo ")"`)x"#, "y"]),
            // Inside double quotes, `$(...)` and backquotes are nested constructs.
            (r#"@("$(echo ")")")x y"#, [r#"@("$(echo ")")")x"#, "y"]),
            (r#"@("`echo "\)"`")x y"#, [r#"@("`echo "\)"`")x"#, "y"]),
            (r#"@("${u:-"}"}")x y"#, [r#"@("${u:-"}"}")x"#, "y"]),
            (r#"@($")")x y"#, [r#"@($")")x"#, "y"]),
            // Nesting still counts unquoted parens; each quote kind hides the other.
            (r#"+(a|@(b|")")|'"') y"#, [r#"+(a|@(b|")")|'"')"#, "y"]),
        ] {
            let tokens = tokenize_str(input)?;
            let token_strs: Vec<_> = tokens.iter().map(Token::to_str).collect();
            assert_eq!(token_strs, expected, "input: {input}");
        }
        Ok(())
    }

    #[test]
    fn tokenize_unterminated_construct_in_extglob() {
        // Each of these leaves a quote, backquote, or `$(` inside the extglob open.
        for input in [r#"@("$(\$"\))x"#, r#"@($"$(\)")x"#, r"@(`)x", r#"@("`)")x"#] {
            assert!(tokenize_str(input).is_err(), "input: {input}");
        }
    }

    #[test]
    fn tokenize_unterminated_extglob() {
        for input in [r"@(a", r#"@(")""#, r"@(')'", r"@(\)"] {
            assert_matches!(
                tokenize_str(input),
                Err(TokenizerError::UnterminatedExtendedGlob(_)),
                "input: {input}"
            );
        }
    }

    #[test]
    fn tokenize_arithmetic_expression() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("a$((1+2))b c")?);
        Ok(())
    }

    #[test]
    fn tokenize_arithmetic_expression_with_space() -> Result<()> {
        // N.B. The spacing comes out a bit odd, but it gets processed okay
        // by later stages.
        assert_ron_snapshot!(test_tokenizer("$(( 1 ))")?);
        Ok(())
    }
    #[test]
    fn tokenize_arithmetic_expression_with_parens() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("$(( (0) ))")?);
        Ok(())
    }

    #[test]
    fn tokenize_special_parameters() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("$$")?);
        assert_ron_snapshot!(test_tokenizer("$@")?);
        assert_ron_snapshot!(test_tokenizer("$!")?);
        assert_ron_snapshot!(test_tokenizer("$?")?);
        assert_ron_snapshot!(test_tokenizer("$*")?);
        Ok(())
    }

    #[test]
    fn tokenize_unbraced_parameter_expansion() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("$x")?);
        assert_ron_snapshot!(test_tokenizer("a$x")?);
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
        assert_ron_snapshot!(test_tokenizer("${x}")?);
        assert_ron_snapshot!(test_tokenizer("a${x}b")?);
        Ok(())
    }

    #[test]
    fn tokenize_braced_parameter_expansion_with_escaping() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r"a${x\}}b")?);
        Ok(())
    }

    #[test]
    fn tokenize_whitespace() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer("1 2 3")?);
        Ok(())
    }

    #[test]
    fn tokenize_escaped_whitespace() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r"1\ 2 3")?);
        Ok(())
    }

    #[test]
    fn tokenize_single_quote() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r"x'a b'y")?);
        Ok(())
    }

    #[test]
    fn tokenize_double_quote() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r#"x"a b"y"#)?);
        Ok(())
    }

    #[test]
    fn tokenize_double_quoted_command_substitution() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r#"x"$(echo hi)"y"#)?);
        Ok(())
    }

    #[test]
    fn tokenize_double_quoted_arithmetic_expression() -> Result<()> {
        assert_ron_snapshot!(test_tokenizer(r#"x"$((1+2))"y"#)?);
        Ok(())
    }

    #[test]
    fn test_quote_removal() {
        assert_eq!(unquote_str(r#""hello""#), "hello");
        assert_eq!(unquote_str(r"'hello'"), "hello");
        assert_eq!(unquote_str(r#""hel\"lo""#), r#"hel"lo"#);
        assert_eq!(unquote_str(r"'hel\'lo'"), r"hel'lo");
    }
}
