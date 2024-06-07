use crate::tokenizer;
use crate::Token;

/// Represents an error that occurred while parsing tokens.
#[derive(Debug)]
pub enum ParseError {
    /// A parsing error occurred near the given token.
    ParsingNearToken(Token),
    /// A parsing error occurred at the end of the input.
    ParsingAtEndOfInput,
    /// An error occurred while tokenizing the input stream.
    Tokenizing {
        /// The inner error.
        inner: tokenizer::TokenizerError,
        /// Optionally provides the position of the error.
        position: Option<tokenizer::SourcePosition>,
    },
}

/// Represents an error that occurred while parsing a word.
#[derive(Debug, thiserror::Error)]
pub enum WordParseError {
    /// An error occurred while parsing an arithmetic expression.
    #[error("failed to parse arithmetic expression")]
    ArithmeticExpression(peg::error::ParseError<peg::str::LineCol>),

    /// An error occurred while parsing a shell pattern.
    #[error("failed to parse pattern")]
    Pattern(peg::error::ParseError<peg::str::LineCol>),

    /// An error occurred while parsing a prompt string.
    #[error("failed to parse prompt string")]
    Prompt(peg::error::ParseError<peg::str::LineCol>),

    /// An error occurred while parsing a parameter.
    #[error("failed to parse parameter '{0}'")]
    Parameter(String, peg::error::ParseError<peg::str::LineCol>),

    /// An error occurred while parsing a word.
    #[error("failed to parse word '{0}'")]
    Word(String, peg::error::ParseError<peg::str::LineCol>),
}

/// Represents an error that occurred while parsing a (non-extended) test command.
#[derive(Debug, thiserror::Error)]
pub enum TestCommandParseError {
    /// An error occurred while parsing a test command.
    #[error("failed to parse test command")]
    TestCommand(peg::error::ParseError<usize>),
}

pub(crate) fn convert_peg_parse_error(
    err: peg::error::ParseError<usize>,
    tokens: &[Token],
) -> ParseError {
    let approx_token_index = err.location;

    if approx_token_index < tokens.len() {
        ParseError::ParsingNearToken(tokens[approx_token_index].clone())
    } else {
        ParseError::ParsingAtEndOfInput
    }
}
