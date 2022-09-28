use anyhow::Result;
use utf8_chars::BufReadCharsExt;

#[derive(Debug, PartialEq)]
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

#[derive(Clone, Debug)]
pub(crate) enum Token {
    Operator(String),
    Word(String),
}

impl Token {
    pub fn to_str(&self) -> &str {
        match self {
            Token::Operator(s) => s,
            Token::Word(s) => s,
        }
    }
}

#[derive(Debug)]
pub(crate) struct TokenizeResult {
    pub reason: TokenEndReason,
    pub token: Option<Token>,
}

#[derive(Debug)]
pub(crate) struct Tokens {
    pub tokens: Vec<Token>,
}

#[derive(Clone, Debug, PartialEq)]
enum QuoteMode {
    None,
    Single,
    Double,
}

#[derive(Clone, Debug, PartialEq)]
struct QuoteState {
    in_escape: bool,
    quote_mode: QuoteMode,
}

impl QuoteState {
    pub fn unquoted(&self) -> bool {
        !self.in_escape && self.quote_mode == QuoteMode::None
    }
}

pub(crate) struct Tokenizer<'a, R: ?Sized + std::io::BufRead> {
    char_reader: std::iter::Peekable<utf8_chars::Chars<'a, R>>,
}

impl<'a, R: ?Sized + std::io::BufRead> Tokenizer<'a, R> {
    pub fn new(reader: &'a mut R) -> Tokenizer<'a, R> {
        Tokenizer {
            char_reader: reader.chars().peekable(),
        }
    }

    fn next_char(&mut self) -> Result<Option<char>> {
        let c = self.char_reader.next().transpose()?;
        Ok(c)
    }

    fn peek_char(&mut self) -> Result<Option<char>> {
        match self.char_reader.peek() {
            Some(result) => match result {
                Ok(c) => Ok(Some(c.clone())),
                Err(_) => Err(anyhow::anyhow!("Failed to decode UTF-8 characters")),
            },
            None => Ok(None),
        }
    }

    pub fn next_token(&mut self) -> Result<TokenizeResult> {
        self.next_token_until(None)
    }

    fn next_token_until(&mut self, terminating_char: Option<char>) -> Result<TokenizeResult> {
        let mut token_so_far = String::new();
        let mut token_is_operator = false;
        let mut quote_state = QuoteState {
            in_escape: false,
            quote_mode: QuoteMode::None,
        };

        loop {
            let mut next_token_is_operator = token_is_operator;
            let mut next_quote_state = quote_state.clone();

            let next = self.peek_char()?;
            let c = next.unwrap_or('\0');

            let mut delimit_token_reason = None;
            let mut include_char = true;
            let mut consume_char = true;

            if next.is_none() {
                // Verify we're out of all quotes.
                if !quote_state.unquoted() {
                    return Err(anyhow::anyhow!("Unterminated quote or escape sequence"));
                }

                delimit_token_reason = Some(TokenEndReason::EndOfInput);
                include_char = false;
            //
            // Look for the specially specified terminating char.
            //
            } else if quote_state.unquoted() && terminating_char == Some(c) {
                delimit_token_reason = Some(TokenEndReason::SpecifiedTerminatingChar);
                include_char = false;
                consume_char = false;
            } else if token_is_operator {
                let mut hypothetical_token = token_so_far.to_owned();
                hypothetical_token.push(c);

                if quote_state.unquoted() && is_operator(hypothetical_token.as_ref()) {
                    // Nothing to do.
                } else {
                    assert!(token_so_far.len() > 0);
                    delimit_token_reason = Some(TokenEndReason::Other);
                }
            } else if does_char_newly_affect_quoting(&quote_state, c) {
                if c == '\\' {
                    // Consume the backslash ourselves so we can peek past it.
                    let _ = self.next_char()?;
                    consume_char = false;

                    if self.peek_char()? == Some('\n') {
                        // Make sure the newline char gets consumed too.
                        consume_char = true;

                        // Make sure to include neither the backslash nor the newline character.
                        include_char = false;
                    } else {
                        next_quote_state.in_escape = true;
                    }
                } else if c == '\'' {
                    //
                    // Enclosing characters in single-quotes ( '' ) shall preserve the literal
                    // value of each character within the single-quotes. A single-quote cannot
                    // occur within single-quotes.
                    //
                    next_quote_state.quote_mode = QuoteMode::Single;
                } else if c == '\"' {
                    next_quote_state.quote_mode = QuoteMode::Double;
                }
            }
            //
            // Handle end of single-quote.
            //
            else if quote_state.quote_mode == QuoteMode::Single
                && !quote_state.in_escape
                && c == '\''
            {
                next_quote_state.quote_mode = QuoteMode::None;
            }
            //
            // Handle end of double-quote.
            //
            else if quote_state.quote_mode == QuoteMode::Double
                && !quote_state.in_escape
                && c == '\"'
            {
                next_quote_state.quote_mode = QuoteMode::None;
            } else if (quote_state.unquoted() || quote_state.quote_mode == QuoteMode::Double)
                && (c == '$' || c == '`')
            {
                // TODO: handle quoted $ or ` in a double quote
                if c == '$' {
                    // First disable normal consumption and consume the '$' char.
                    consume_char = false;
                    include_char = false;
                    let _ = self.next_char()?;

                    // Add the opening '$' to the token.
                    token_so_far.push(c);

                    // Now peek beyond to see what we have.
                    let char_after_dollar_sign = self.peek_char()?;
                    if let Some(cads) = char_after_dollar_sign {
                        match cads {
                            '(' => {
                                // Consume the '(' and add it to the token.
                                token_so_far.push(self.next_char()?.unwrap());

                                loop {
                                    let cur_token = self.next_token_until(Some(')'))?;
                                    if let Some(cur_token_value) = cur_token.token {
                                        token_so_far.push_str(cur_token_value.to_str())
                                    }

                                    if cur_token.reason == TokenEndReason::NonNewLineBlank {
                                        token_so_far.push_str(" ");
                                    }

                                    if cur_token.reason == TokenEndReason::SpecifiedTerminatingChar
                                    {
                                        // We hit the ')' we were looking for but did not
                                        // yet consume it. Do so now.
                                        token_so_far.push(self.next_char()?.unwrap());
                                        break;
                                    }
                                }
                            }
                            _ => {
                                if cads == '{' {
                                    // Consume the '{' and add it to the token.
                                    token_so_far.push(self.next_char()?.unwrap());

                                    loop {
                                        let cur_token = self.next_token_until(Some('}'))?;
                                        if let Some(cur_token_value) = cur_token.token {
                                            token_so_far.push_str(cur_token_value.to_str())
                                        }

                                        if cur_token.reason == TokenEndReason::NonNewLineBlank {
                                            token_so_far.push_str(" ");
                                        }

                                        if cur_token.reason
                                            == TokenEndReason::SpecifiedTerminatingChar
                                        {
                                            // We hit the end brace we were looking for but did not
                                            // yet consume it. Do so now.
                                            token_so_far.push(self.next_char()?.unwrap());
                                            break;
                                        }
                                    }
                                } else {
                                    //
                                    // Nothing to do.
                                    //
                                }
                            }
                        }
                    }
                } else {
                    // We look for the terminating backquote. First disable normal consumption and consume
                    // the starting backquote.
                    consume_char = false;
                    include_char = false;
                    let _ = self.next_char()?;

                    // Add the opening backquote to the token.
                    token_so_far.push(c);

                    // Now continue until we see an unescaped backquote.
                    let mut escaping_enabled = false;
                    let mut done = false;
                    while !done {
                        // Read (and consume) the next char.
                        let next_char_in_backquote = self.next_char()?;
                        if let Some(cib) = next_char_in_backquote {
                            // Include it in the token no matter what.
                            token_so_far.push(cib);

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
                            return Err(anyhow::anyhow!("Unterminated backquote"));
                        }
                    }
                }
            } else if quote_state.unquoted() && can_start_operator(c) {
                if token_so_far.len() > 0 {
                    delimit_token_reason = Some(TokenEndReason::Other);
                }
                next_token_is_operator = true;
            } else if quote_state.unquoted() && is_blank(c) {
                if token_so_far.len() > 0 {
                    delimit_token_reason = Some(TokenEndReason::NonNewLineBlank);
                }
                include_char = false;
            } else if !token_is_operator && token_so_far.len() > 0 {
                // Nothing to do.
            } else if c == '#' {
                let mut done = false;
                while !done {
                    done = match self.peek_char()? {
                        Some('\n') => true,
                        None => true,
                        _ => {
                            // Consume the peeked char; it's part of the comment.
                            let _ = self.next_char()?;
                            false
                        }
                    };
                }

                // Re-start loop as if the comment never happened.
                continue;
            } else {
                if token_so_far.len() > 0 {
                    delimit_token_reason = Some(TokenEndReason::Other);
                }
            }

            //
            // Now process what we decided.
            //

            if let Some(reason) = delimit_token_reason {
                let token = if token_so_far.len() > 0 {
                    if token_is_operator {
                        Some(Token::Operator(token_so_far))
                    } else {
                        Some(Token::Word(token_so_far))
                    }
                } else {
                    None
                };

                return Ok(TokenizeResult { reason, token });
            }

            // Consume the char.
            if consume_char {
                let _ = self.next_char()?;
            }

            if include_char {
                // ...and append it to our in-progress token if so requested.
                token_so_far.push(c);
            }

            // Update our tracking of whether the current token is an operator.
            token_is_operator = next_token_is_operator;

            // Update quote state. Escaping only lasts one character.
            if quote_state.in_escape {
                next_quote_state.in_escape = false;
            }
            quote_state = next_quote_state;
        }
    }
}

impl<'a, R: ?Sized + std::io::BufRead> Iterator for Tokenizer<'a, R> {
    type Item = Result<TokenizeResult>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_token() {
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

fn can_start_operator(c: char) -> bool {
    matches!(c, '&' | '(' | ')' | ';' | '\n' | '|' | '<' | '>')
}

fn does_char_newly_affect_quoting(quote_state: &QuoteState, c: char) -> bool {
    // If we're currently escaped, then nothing affects quoting.
    if quote_state.in_escape {
        return false;
    }

    match quote_state.quote_mode {
        // When we're in a double quote, only a subset of escape sequences are recognized.
        QuoteMode::Double => {
            if c == '\\' {
                // TODO: handle backslash in double quote
                true
            } else {
                false
            }
        }
        // When we're in a single quote, nothing affects quoting.
        QuoteMode::Single => false,
        // When we're not already in a quote, then we can straightforwardly look for a
        // quote mark or backslash.
        QuoteMode::None => is_quoting_char(c),
    }
}

fn is_quoting_char(c: char) -> bool {
    matches!(c, '\\' | '\'' | '\"')
}

fn is_operator(s: &str) -> bool {
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
