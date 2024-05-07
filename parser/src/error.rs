use crate::tokenizer;
use crate::Token;

#[derive(Debug)]
pub enum ParseError {
    ParsingNearToken(Token),
    ParsingAtEndOfInput,
    Tokenizing {
        inner: tokenizer::TokenizerError,
        position: Option<tokenizer::SourcePosition>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum WordParseError {
    #[error("failed to parse arithmetic expression")]
    ArithmeticExpression(peg::error::ParseError<peg::str::LineCol>),

    #[error("failed to parse pattern")]
    Pattern(peg::error::ParseError<peg::str::LineCol>),

    #[error("failed to parse prompt string")]
    Prompt(peg::error::ParseError<peg::str::LineCol>),

    #[error("failed to parse parameter '{0}'")]
    Parameter(String, peg::error::ParseError<peg::str::LineCol>),

    #[error("failed to parse word '{0}'")]
    Word(String, peg::error::ParseError<peg::str::LineCol>),
}

#[derive(Debug, thiserror::Error)]
pub enum TestCommandParseError {
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
