use crate::ast;
use crate::tokenizer::{Token, TokenEndReason, Tokenizer, TokenizerOptions, Tokens};

use bon::bon;

pub mod peg;

/// Parser implementation to use
#[derive(Clone, Eq, Hash, PartialEq, Default)]
pub enum ParserImpl {
    /// PEG-based parser
    #[default]
    Peg,
}

/// Options used to control the behavior of the parser.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct ParserOptions {
    /// Whether or not to enable extended globbing (a.k.a. `extglob`).
    pub enable_extended_globbing: bool,
    /// Whether or not to enable POSIX compliance mode.
    pub posix_mode: bool,
    /// Whether or not to enable maximal compatibility with the `sh` shell.
    pub sh_mode: bool,
    /// Whether or not to perform tilde expansion for tildes at the start of words.
    pub tilde_expansion_at_word_start: bool,
    /// Whether or not to perform tilde expansion for tildes after colons.
    pub tilde_expansion_after_colon: bool,
    /// Select the parser internal implementation
    pub parser_impl: ParserImpl,
}

impl Default for ParserOptions {
    fn default() -> Self {
        Self {
            enable_extended_globbing: true,
            posix_mode: false,
            sh_mode: false,
            tilde_expansion_at_word_start: true,
            tilde_expansion_after_colon: false,
            parser_impl: ParserImpl::default(),
        }
    }
}

impl ParserOptions {
    /// Returns the tokenizer options implied by these parser options.
    pub const fn tokenizer_options(&self) -> TokenizerOptions {
        TokenizerOptions {
            enable_extended_globbing: self.enable_extended_globbing,
            posix_mode: self.posix_mode,
            sh_mode: self.sh_mode,
        }
    }
}

/// Implements parsing for shell programs.
pub struct Parser<R: std::io::BufRead> {
    /// The reader to use for input
    reader: R,
    /// Parsing options
    options: ParserOptions,
}

#[bon]
impl<R: std::io::BufRead> Parser<R> {
    ///
    /// # Arguments
    ///
    /// * `reader` - The reader to use for input.
    /// * `options` - The options to use when parsing.
    pub fn new(reader: R, options: &ParserOptions) -> Self {
        Self {
            reader,
            options: options.clone(),
        }
    }

    /// Create a new parser instance through a builder
    #[builder(
        finish_fn(doc {
            /// Instantiate a parser with the provided reader as input
        })
    )]
    pub const fn builder(
        /// The reader to use for input
        #[builder(finish_fn)]
        reader: R,

        #[builder(default = true)]
        /// Whether or not to enable extended globbing (a.k.a. `extglob`).
        enable_extended_globbing: bool,
        #[builder(default = false)]
        /// Whether or not to enable POSIX compliance mode.
        posix_mode: bool,
        #[builder(default = false)]
        /// Whether or not to enable maximal compatibility with the `sh` shell.
        sh_mode: bool,
        #[builder(default = true)]
        /// Whether or not to perform tilde expansion for tildes at the start of words.
        tilde_expansion_at_word_start: bool,
        #[builder(default = false)]
        /// Whether or not to perform tilde expansion for tildes after colons.
        tilde_expansion_after_colon: bool,
        #[builder(default)]
        /// Select the parser internal implementation
        parser_impl: ParserImpl,
    ) -> Self {
        let options = ParserOptions {
            enable_extended_globbing,
            posix_mode,
            sh_mode,
            tilde_expansion_at_word_start,
            tilde_expansion_after_colon,
            parser_impl,
        };
        Self { reader, options }
    }

    /// Parses the input into an abstract syntax tree (AST) of a shell program.
    pub fn parse_program(&mut self) -> Result<ast::Program, crate::error::ParseError> {
        //
        // References:
        //   * https://www.gnu.org/software/bash/manual/bash.html#Shell-Syntax
        //   * https://mywiki.wooledge.org/BashParser
        //   * https://aosabook.org/en/v1/bash.html
        //   * https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html
        //

        let tokens = self.tokenize()?;
        parse_tokens(&tokens, &self.options)
    }

    /// Parses a function definition body from the input. The body is expected to be
    /// preceded by "()", but no function name.
    pub fn parse_function_parens_and_body(
        &mut self,
    ) -> Result<ast::FunctionBody, crate::error::ParseError> {
        let tokens = self.tokenize()?;
        let parse_result =
            peg::token_parser::function_parens_and_body(&Tokens { tokens: &tokens }, &self.options);
        parse_result_to_error(parse_result, &tokens)
    }

    fn tokenize(&mut self) -> Result<Vec<Token>, crate::error::ParseError> {
        // First we tokenize the input, according to the policy implied by provided options.
        let mut tokenizer = Tokenizer::new(&mut self.reader, &self.options.tokenizer_options());

        tracing::debug!(target: "tokenize", "Tokenizing...");

        let mut tokens = vec![];
        loop {
            let result = match tokenizer.next_token() {
                Ok(result) => result,
                Err(e) => {
                    return Err(crate::error::ParseError::Tokenizing {
                        inner: e,
                        position: tokenizer.current_location(),
                    });
                }
            };

            let reason = result.reason;
            if let Some(token) = result.token {
                tracing::debug!(target: "tokenize", "TOKEN {}: {:?} {reason:?}", tokens.len(), token);
                tokens.push(token);
            }

            if matches!(reason, TokenEndReason::EndOfInput) {
                break;
            }
        }

        tracing::debug!(target: "tokenize", "  => {} token(s)", tokens.len());

        Ok(tokens)
    }
}

/// Parses a sequence of tokens into the abstract syntax tree (AST) of a shell program.
///
/// # Arguments
///
/// * `tokens` - The tokens to parse.
/// * `options` - The options to use when parsing.
pub fn parse_tokens(
    tokens: &[Token],
    options: &ParserOptions,
) -> Result<ast::Program, crate::error::ParseError> {
    let parse_result = peg::token_parser::program(&Tokens { tokens }, options);
    parse_result_to_error(parse_result, tokens)
}

fn parse_result_to_error<R>(
    parse_result: Result<R, ::peg::error::ParseError<usize>>,
    tokens: &[Token],
) -> Result<R, crate::error::ParseError>
where
    R: std::fmt::Debug,
{
    match parse_result {
        Ok(program) => {
            tracing::debug!(target: "parse", "PROG: {:?}", program);
            Ok(program)
        }
        Err(parse_error) => {
            tracing::debug!(target: "parse", "Parse error: {:?}", parse_error);
            Err(crate::error::convert_peg_parse_error(&parse_error, tokens))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use insta::assert_ron_snapshot;

    #[derive(serde::Serialize)]
    struct ParseResult<'a, T> {
        input: &'a str,
        result: &'a T,
    }

    #[test]
    fn parse_case() -> Result<()> {
        let input = r"\
case x in
x)
    echo y;;
esac\
";

        let mut parser = Parser::builder()
            .sh_mode(false)
            .build(std::io::Cursor::new(input));
        let result = parser.parse_program()?;

        assert_ron_snapshot!(ParseResult {
            input,
            result: &result
        });

        Ok(())
    }

    #[test]
    fn parse_case_ns() -> Result<()> {
        let input = r"\
case x in
x)
    echo y
esac\
";

        let mut parser = Parser::builder()
            .sh_mode(false)
            .build(std::io::Cursor::new(input));
        let result = parser.parse_program()?;

        assert_ron_snapshot!(ParseResult {
            input,
            result: &result
        });

        Ok(())
    }

    #[test]
    fn parse_arith_and_non_arith_parens() -> Result<()> {
        let input = r"( : && ( (( 0 )) || : ) )";

        let mut parser = Parser::builder()
            .sh_mode(false)
            .build(std::io::Cursor::new(input));
        let result = parser.parse_program()?;

        assert_ron_snapshot!(ParseResult {
            input,
            result: &result
        });

        Ok(())
    }

    #[test]
    fn parse_redirection() -> Result<()> {
        let input = r"echo |& wc";

        let mut parser = Parser::builder()
            .sh_mode(false)
            .build(std::io::Cursor::new(input));
        let result = parser.parse_program()?;

        assert_ron_snapshot!(ParseResult {
            input,
            result: &result
        });

        Ok(())
    }

    #[test]
    fn parse_here_doc_with_no_trailing_newline() -> Result<()> {
        let input = r"cat <<EOF
Something
EOF";

        let mut parser = Parser::builder()
            .sh_mode(false)
            .build(std::io::Cursor::new(input));
        let result = parser.parse_program()?;

        assert_ron_snapshot!(ParseResult {
            input,
            result: &result
        });

        Ok(())
    }

    #[test]
    fn parse_function_with_pipe_redirection() -> Result<()> {
        let inputs = [r"foo() { echo 1; } 2>&1 | cat", r"foo() { echo 1; } |& cat"];

        for input in inputs {
            let mut parser = Parser::builder()
                .sh_mode(false)
                .build(std::io::Cursor::new(input));
            let result = parser.parse_program()?;

            assert_ron_snapshot!(ParseResult {
                input,
                result: &result
            });
        }

        Ok(())
    }

    #[test]
    fn test_parse_program() -> Result<()> {
        let input = r#"

#!/usr/bin/env bash

for f in A B C; do

    # sdfsdf
    echo "${f@L}" >&2

   done

"#;

        let mut parser = Parser::builder()
            .sh_mode(false)
            .build(std::io::Cursor::new(input));
        let result = parser.parse_program()?;

        assert_ron_snapshot!(ParseResult {
            input,
            result: &result
        });

        Ok(())
    }
}
