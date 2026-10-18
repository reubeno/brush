//! The word grammar of the winnow parser: the same pieces as the PEG grammar in the parent
//! module, in the same order, read without backtracking where the next byte decides.
//!
//! Each function takes the rest of the input, `&mut &str`, and leaves it past what it read. A
//! failed alternative leaves it where it was. Offsets in [`WordPieceWithSource`] count bytes from
//! the start of the word, as in the PEG grammar.

#![allow(
    clippy::string_slice,
    reason = "every offset is that of an ASCII byte found by scanning, so a char boundary"
)]

use super::{
    BraceExpression, BraceExpressionMember, BraceExpressionOrText, Parameter, ParameterExpr,
    ParameterTestType, ParameterTransformOp, SpecialParameter, SubstringMatchKind, TildeExpr,
    WordPiece, WordPieceWithSource, parse_sequence_bound, zero_padded_width,
};
use crate::{ParserOptions, ast};

/// Where a piece of text stops, besides the end of the input.
#[derive(Clone, Copy)]
enum Stop {
    /// At the end of the input only.
    End,
    /// At `}`.
    Brace,
    /// At `}` or `/`.
    BraceOrSlash,
    /// At `{`, or where `inner` stops.
    OpenBrace(&'static Self),
    /// At `,` or `}`.
    CommaOrBrace,
    /// At `))`.
    DoubleParen,
    /// At `)`.
    Paren,
    /// At `]`.
    Bracket,
    /// At `:` or `}`.
    ColonOrBrace,
    /// Where `inner` stops, or at `(`.
    OrOpenParen(&'static Self),
}

impl Stop {
    fn at(self, rest: &str) -> bool {
        match self {
            Self::End => false,
            Self::Brace => rest.starts_with('}'),
            Self::BraceOrSlash => rest.starts_with(['}', '/']),
            Self::OpenBrace(inner) => rest.starts_with('{') || inner.at(rest),
            Self::CommaOrBrace => rest.starts_with([',', '}']),
            Self::DoubleParen => rest.starts_with("))"),
            Self::Paren => rest.starts_with(')'),
            Self::Bracket => rest.starts_with(']'),
            Self::ColonOrBrace => rest.starts_with([':', '}']),
            Self::OrOpenParen(inner) => inner.at(rest) || rest.starts_with('('),
        }
    }

    /// The same stop with `(` added, for an arithmetic word's text.
    const fn or_open_paren(self) -> Self {
        match self {
            Self::End => Self::OrOpenParen(&Self::End),
            Self::Brace => Self::OrOpenParen(&Self::Brace),
            Self::DoubleParen => Self::OrOpenParen(&Self::DoubleParen),
            Self::Paren => Self::OrOpenParen(&Self::Paren),
            Self::Bracket => Self::OrOpenParen(&Self::Bracket),
            Self::ColonOrBrace => Self::OrOpenParen(&Self::ColonOrBrace),
            other => other,
        }
    }
}

/// A failed alternative.
struct Fail;

type Res<T> = Result<T, Fail>;

/// The grammar's state: the whole word, for offsets and the character before a piece.
struct Grammar<'a> {
    word: &'a str,
    options: &'a ParserOptions,
    /// The last `$` piece parsed to see where text ends: its start, its end, the piece. The
    /// piece that follows the text is that one, and is not parsed again.
    last_dollar: std::cell::RefCell<Option<(usize, usize, WordPiece)>>,
}

/// Runs `f` on a copy of the input, keeping what it read only when it succeeds.
fn attempt<'a, T>(input: &mut &'a str, f: impl FnOnce(&mut &'a str) -> Res<T>) -> Res<T> {
    let mut copy = *input;
    let result = f(&mut copy)?;
    *input = copy;
    Ok(result)
}

fn literal(input: &mut &str, s: &str) -> Res<()> {
    match input.strip_prefix(s) {
        Some(rest) => {
            *input = rest;
            Ok(())
        }
        None => Err(Fail),
    }
}

fn next_char(input: &mut &str) -> Res<char> {
    let c = input.chars().next().ok_or(Fail)?;
    *input = &input[c.len_utf8()..];
    Ok(c)
}

const fn is_name_char(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

impl<'a> Grammar<'a> {
    const fn offset(&self, input: &str) -> usize {
        self.word.len() - input.len()
    }

    // ---- Words --------------------------------------------------------------------------

    /// A word: a tilde prefix if enabled, then pieces until `stop`.
    fn word(&self, input: &mut &'a str, stop: Stop) -> Vec<WordPieceWithSource> {
        let mut pieces = Vec::new();
        if self.options.tilde_expansion_at_word_start {
            let start = self.offset(input);
            if let Ok(piece) = attempt(input, |i| self.tilde_expression_piece(i)) {
                pieces.push(WordPieceWithSource {
                    piece,
                    start_index: start,
                    end_index: self.offset(input),
                });
            }
        }
        while let Ok(piece) = self.word_piece_with_source(input, stop) {
            pieces.push(piece);
        }
        pieces
    }

    fn word_piece_with_source(&self, input: &mut &'a str, stop: Stop) -> Res<WordPieceWithSource> {
        let start = self.offset(input);
        let piece = self.word_piece(input, stop)?;
        Ok(WordPieceWithSource {
            piece,
            start_index: start,
            end_index: self.offset(input),
        })
    }

    fn word_piece(&self, input: &mut &'a str, stop: Stop) -> Res<WordPiece> {
        let first = *input.as_bytes().first().ok_or(Fail)?;
        match first {
            b'"' => {
                if let Ok(s) = attempt(input, |i| self.double_quoted_sequence(i, "\"")) {
                    return Ok(WordPiece::DoubleQuotedSequence(s));
                }
            }
            b'\'' => {
                if let Ok(s) = attempt(input, single_quoted_literal_text) {
                    return Ok(WordPiece::SingleQuotedText(s.to_owned()));
                }
            }
            b'$' => {
                if input.starts_with("$'") {
                    if let Ok(s) = attempt(input, ansi_c_quoted_text) {
                        return Ok(WordPiece::AnsiCQuotedText(s.to_owned()));
                    }
                }
                if input.starts_with("$\"") {
                    if let Ok(s) = attempt(input, |i| self.double_quoted_sequence(i, "$\"")) {
                        return Ok(WordPiece::GettextDoubleQuotedSequence(s));
                    }
                }
                if let Ok(piece) = attempt(input, |i| self.dollar_sign_word_piece(i)) {
                    return Ok(piece);
                }
            }
            b'`' => {
                if let Ok(piece) = attempt(input, |i| self.backquoted(i)) {
                    return Ok(piece);
                }
            }
            b'\\' => {
                if let Ok(piece) = attempt(input, normal_escape_sequence) {
                    return Ok(piece);
                }
            }
            _ => {}
        }
        if let Ok(piece) = attempt(input, |i| self.tilde_expr_after_colon(i)) {
            return Ok(piece);
        }
        self.unquoted_literal_text(input, stop)
    }

    fn unquoted_literal_text(&self, input: &mut &'a str, stop: Stop) -> Res<WordPiece> {
        let start = *input;
        loop {
            let rest = *input;
            if rest.is_empty() || stop.at(rest) {
                break;
            }
            let b = rest.as_bytes()[0];
            if matches!(b, b'\'' | b'"' | b'$' | b'`') {
                break;
            }
            // An escape is a piece of its own.
            if b == b'\\' && rest.len() > 1 {
                break;
            }
            if rest.len() < start.len() && self.tilde_after_colon_here(rest) {
                break;
            }
            let _ = next_char(input);
        }
        let len = start.len() - input.len();
        if len == 0 {
            return Err(Fail);
        }
        Ok(WordPiece::Text(start[..len].to_owned()))
    }

    // ---- Quotes -------------------------------------------------------------------------

    /// `"…"` or `$"…"`: the pieces between the quotes.
    fn double_quoted_sequence(
        &self,
        input: &mut &'a str,
        open: &str,
    ) -> Res<Vec<WordPieceWithSource>> {
        literal(input, open)?;
        let mut pieces = Vec::new();
        while !input.starts_with('"') {
            let start = self.offset(input);
            let piece = self.double_quoted_word_piece(input)?;
            pieces.push(WordPieceWithSource {
                piece,
                start_index: start,
                end_index: self.offset(input),
            });
        }
        literal(input, "\"")?;
        Ok(pieces)
    }

    fn double_quoted_word_piece(&self, input: &mut &'a str) -> Res<WordPiece> {
        if input.starts_with('$') {
            if let Ok(piece) = attempt(input, |i| self.dollar_sign_word_piece(i)) {
                return Ok(piece);
            }
        }
        if input.starts_with('`') {
            if let Ok(piece) = attempt(input, |i| self.backquoted(i)) {
                return Ok(piece);
            }
        }
        if let Ok(piece) = attempt(input, |i| escape_in(i, b"$`\"\\")) {
            return Ok(piece);
        }
        // Text, up to an escape, an expansion, or the closing quote.
        let start = *input;
        loop {
            let rest = *input;
            let Some(&b) = rest.as_bytes().first() else {
                break;
            };
            if b == b'"' {
                break;
            }
            if b == b'\\'
                && rest
                    .as_bytes()
                    .get(1)
                    .is_some_and(|c| b"$`\"\\".contains(c))
            {
                break;
            }
            if b == b'`' && attempt(&mut { rest }, |i| self.backquoted(i)).is_ok() {
                break;
            }
            if b == b'$' && (self.starts_dollar_piece(rest) || is_expansion_opener(rest)) {
                break;
            }
            let _ = next_char(input);
        }
        let len = start.len() - input.len();
        if len == 0 {
            return Err(Fail);
        }
        Ok(WordPiece::Text(start[..len].to_owned()))
    }

    /// Whether a `$` piece parses at the start of `rest`.
    fn starts_dollar_piece(&self, rest: &'a str) -> bool {
        let mut copy = rest;
        match self.dollar_sign_word_piece(&mut copy) {
            Ok(piece) => {
                *self.last_dollar.borrow_mut() =
                    Some((self.offset(rest), self.offset(copy), piece));
                true
            }
            Err(Fail) => false,
        }
    }

    // ---- Dollar pieces ------------------------------------------------------------------

    fn dollar_sign_word_piece(&self, input: &mut &'a str) -> Res<WordPiece> {
        let start = self.offset(input);
        let remembered = self.last_dollar.borrow_mut().take();
        if let Some((at, end, piece)) = remembered
            && at == start
        {
            *input = &input[end - start..];
            return Ok(piece);
        }
        if input.starts_with("$((") {
            if let Ok(piece) = attempt(input, |i| self.arithmetic_expansion(i)) {
                return Ok(piece);
            }
        }
        if input.starts_with("$[") {
            if let Ok(piece) = attempt(input, |i| self.legacy_arithmetic_expansion(i)) {
                return Ok(piece);
            }
        }
        if input.starts_with("$(") {
            if let Ok(piece) = attempt(input, |i| self.command_substitution(i)) {
                return Ok(piece);
            }
        }
        self.parameter_expansion(input)
    }

    fn arithmetic_expansion(&self, input: &mut &'a str) -> Res<WordPiece> {
        literal(input, "$((")?;
        let start = *input;
        self.arithmetic_word(input, Stop::DoubleParen);
        let value = &start[..start.len() - input.len()];
        literal(input, "))")?;
        Ok(WordPiece::ArithmeticExpression(
            ast::UnexpandedArithmeticExpr {
                value: value.to_owned(),
            },
        ))
    }

    fn legacy_arithmetic_expansion(&self, input: &mut &'a str) -> Res<WordPiece> {
        literal(input, "$[")?;
        let body = crate::tokenizer::legacy_arithmetic_expansion_body(
            input,
            &self.options.tokenizer_options(),
        )
        .map_err(|_| Fail)?;
        *input = &input[body.len()..];
        literal(input, "]")?;
        Ok(WordPiece::ArithmeticExpression(
            ast::UnexpandedArithmeticExpr {
                value: body.to_owned(),
            },
        ))
    }

    fn command_substitution(&self, input: &mut &'a str) -> Res<WordPiece> {
        literal(input, "$(")?;
        let body =
            crate::tokenizer::command_substitution_body(input, &self.options.tokenizer_options())
                .map_err(|_| Fail)?;
        *input = &input[body.len()..];
        literal(input, ")")?;
        Ok(WordPiece::CommandSubstitution(body.to_owned()))
    }

    /// `` `…` ``: a backslash is removed only before `$`, `` ` `` or `\`.
    fn backquoted(&self, input: &mut &'a str) -> Res<WordPiece> {
        let _ = self;
        literal(input, "`")?;
        let mut command = String::new();
        loop {
            let rest = *input;
            let bytes = rest.as_bytes();
            match bytes.first() {
                None => return Err(Fail),
                Some(b'`') => break,
                Some(b'\\') if bytes.get(1).is_some_and(|c| b"$`\\".contains(c)) => {
                    command.push(char::from(bytes[1]));
                    *input = &rest[2..];
                }
                Some(_) => command.push(next_char(input)?),
            }
        }
        literal(input, "`")?;
        // A backslash ending the input stays literal; see the PEG grammar.
        if command.chars().rev().take_while(|c| *c == '\\').count() % 2 == 1 {
            command.push('\\');
        }
        Ok(WordPiece::BackquotedCommandSubstitution(command))
    }

    fn parameter_expansion(&self, input: &mut &'a str) -> Res<WordPiece> {
        if input.starts_with("${") {
            let parsed = attempt(input, |i| {
                literal(i, "${")?;
                let expr = self.parameter_expression(i)?;
                literal(i, "}")?;
                Ok(expr)
            });
            if let Ok(expr) = parsed {
                return Ok(WordPiece::ParameterExpansion(expr));
            }
        }
        if let Ok(parameter) = attempt(input, |i| {
            literal(i, "$")?;
            self.unbraced_parameter(i)
        }) {
            return Ok(WordPiece::ParameterExpansion(ParameterExpr::Parameter {
                parameter,
                indirect: false,
            }));
        }
        if input.starts_with('$') && !is_expansion_opener(input) && !input.starts_with("$'") {
            *input = &input[1..];
            return Ok(WordPiece::Text("$".to_owned()));
        }
        Err(Fail)
    }

    // ---- Parameter expressions ----------------------------------------------------------

    /// The inside of `${…}`, its forms tried in the PEG grammar's order: the first that
    /// matches is taken, whether or not the `}` follows. Most forms start with a parameter,
    /// which is parsed once; those that do not are tried at their place in that order.
    #[allow(
        clippy::too_many_lines,
        reason = "one arm per form, in the PEG grammar's order"
    )]
    fn parameter_expression(&self, input: &mut &'a str) -> Res<ParameterExpr> {
        let mut after = *input;
        let first = self
            .indirect_parameter(&mut after)
            .ok()
            .map(|(indirect, parameter)| (indirect, parameter, after));
        // A form that starts with the parameter: on success the input goes past it.
        let form = |input: &mut &'a str,
                    f: &dyn Fn(&mut &'a str, bool, Parameter) -> Res<ParameterExpr>|
         -> Option<ParameterExpr> {
            let (indirect, parameter, after) = first.as_ref()?;
            let mut rest = *after;
            let expr = f(&mut rest, *indirect, parameter.clone()).ok()?;
            *input = rest;
            Some(expr)
        };

        // `${p-w}`, `${p=w}`, `${p?w}`, `${p+w}`, with or without a colon.
        if let Some(e) = form(input, &|i, indirect, parameter| {
            let test_type = if i.starts_with(':') {
                *i = &i[1..];
                ParameterTestType::UnsetOrNull
            } else {
                ParameterTestType::Unset
            };
            let op = *i.as_bytes().first().ok_or(Fail)?;
            if !matches!(op, b'-' | b'=' | b'?' | b'+') {
                return Err(Fail);
            }
            *i = &i[1..];
            let value = Some(self.text_of_word(i, Stop::Brace));
            Ok(match op {
                b'-' => ParameterExpr::UseDefaultValues {
                    parameter,
                    indirect,
                    test_type,
                    default_value: value,
                },
                b'=' => ParameterExpr::AssignDefaultValues {
                    parameter,
                    indirect,
                    test_type,
                    default_value: value,
                },
                b'?' => ParameterExpr::IndicateErrorIfNullOrUnset {
                    parameter,
                    indirect,
                    test_type,
                    error_message: value,
                },
                _ => ParameterExpr::UseAlternativeValue {
                    parameter,
                    indirect,
                    test_type,
                    alternative_value: value,
                },
            })
        }) {
            return Ok(e);
        }
        // `${#p}`
        if input.starts_with('#')
            && let Ok(parameter) = attempt(input, |i| {
                literal(i, "#")?;
                self.parameter(i)
            })
        {
            return Ok(ParameterExpr::ParameterLength {
                parameter,
                indirect: false,
            });
        }
        // `${p%%w}`, `${p%w}`, `${p##w}`, `${p#w}`
        for (op, make) in [("%%", 0u8), ("%", 1), ("##", 2), ("#", 3)] {
            if let Some(e) = form(input, &|i, indirect, parameter| {
                literal(i, op)?;
                let pattern = Some(self.text_of_word(i, Stop::Brace));
                Ok(match make {
                    0 => ParameterExpr::RemoveLargestSuffixPattern {
                        parameter,
                        indirect,
                        pattern,
                    },
                    1 => ParameterExpr::RemoveSmallestSuffixPattern {
                        parameter,
                        indirect,
                        pattern,
                    },
                    2 => ParameterExpr::RemoveLargestPrefixPattern {
                        parameter,
                        indirect,
                        pattern,
                    },
                    _ => ParameterExpr::RemoveSmallestPrefixPattern {
                        parameter,
                        indirect,
                        pattern,
                    },
                })
            }) {
                return Ok(e);
            }
        }
        if !self.options.sh_mode {
            // `${!name[*]}`, `${!name[@]}`
            for (suffix, concatenate) in [("[*]", true), ("[@]", false)] {
                if input.starts_with('!')
                    && let Ok(name) = attempt(input, |i| {
                        literal(i, "!")?;
                        let name = variable_name(i)?;
                        literal(i, suffix)?;
                        Ok(name)
                    })
                {
                    return Ok(ParameterExpr::MemberKeys {
                        variable_name: name.to_owned(),
                        concatenate,
                    });
                }
            }
            // `${p:offset}`, `${p:offset:length}`
            if let Some(e) = form(input, &|i, indirect, parameter| {
                literal(i, ":")?;
                let offset = self.arithmetic_text(i, Stop::ColonOrBrace);
                let length = attempt(i, |i| {
                    literal(i, ":")?;
                    Ok(self.arithmetic_text(i, Stop::ColonOrBrace))
                })
                .ok();
                Ok(ParameterExpr::Substring {
                    parameter,
                    indirect,
                    offset,
                    length,
                })
            }) {
                return Ok(e);
            }
            // `${p@op}`
            if let Some(e) = form(input, &|i, indirect, parameter| {
                literal(i, "@")?;
                let op = match next_char(i)? {
                    'U' => ParameterTransformOp::ToUpperCase,
                    'u' => ParameterTransformOp::CapitalizeInitial,
                    'L' => ParameterTransformOp::ToLowerCase,
                    'Q' => ParameterTransformOp::Quoted,
                    'E' => ParameterTransformOp::ExpandEscapeSequences,
                    'P' => ParameterTransformOp::PromptExpand,
                    'A' => ParameterTransformOp::ToAssignmentLogic,
                    'K' => ParameterTransformOp::PossiblyQuoteWithArraysExpanded {
                        separate_words: false,
                    },
                    'a' => ParameterTransformOp::ToAttributeFlags,
                    'k' => ParameterTransformOp::PossiblyQuoteWithArraysExpanded {
                        separate_words: true,
                    },
                    _ => return Err(Fail),
                };
                Ok(ParameterExpr::Transform {
                    parameter,
                    indirect,
                    op,
                })
            }) {
                return Ok(e);
            }
            // `${!prefix*}`, `${!prefix@}`
            for (suffix, concatenate) in [("*", true), ("@", false)] {
                if input.starts_with('!')
                    && let Ok(prefix) = attempt(input, |i| {
                        literal(i, "!")?;
                        let prefix = variable_name(i)?;
                        literal(i, suffix)?;
                        Ok(prefix)
                    })
                {
                    return Ok(ParameterExpr::VariableNames {
                        prefix: prefix.to_owned(),
                        concatenate,
                    });
                }
            }
            // `${p/#pat/rep}`, `${p/%pat/rep}`, `${p//pat/rep}`, `${p/pat/rep}`
            for (op, match_kind) in [
                ("/#", SubstringMatchKind::Prefix),
                ("/%", SubstringMatchKind::Suffix),
                ("//", SubstringMatchKind::Anywhere),
                ("/", SubstringMatchKind::FirstOccurrence),
            ] {
                if let Some(e) = form(input, &|i, indirect, parameter| {
                    literal(i, op)?;
                    let pattern = self.text_of_word(i, Stop::BraceOrSlash);
                    let replacement = attempt(i, |i| {
                        literal(i, "/")?;
                        Ok(self.text_of_word(i, Stop::Brace))
                    })
                    .ok();
                    Ok(ParameterExpr::ReplaceSubstring {
                        parameter,
                        indirect,
                        pattern,
                        replacement,
                        match_kind: match_kind.clone(),
                    })
                }) {
                    return Ok(e);
                }
            }
            // `${p^^w}`, `${p^w}`, `${p,,w}`, `${p,w}`
            for (op, make) in [("^^", 0u8), ("^", 1), (",,", 2), (",", 3)] {
                if let Some(e) = form(input, &|i, indirect, parameter| {
                    literal(i, op)?;
                    let pattern = Some(self.text_of_word(i, Stop::Brace));
                    Ok(match make {
                        0 => ParameterExpr::UppercasePattern {
                            parameter,
                            indirect,
                            pattern,
                        },
                        1 => ParameterExpr::UppercaseFirstChar {
                            parameter,
                            indirect,
                            pattern,
                        },
                        2 => ParameterExpr::LowercasePattern {
                            parameter,
                            indirect,
                            pattern,
                        },
                        _ => ParameterExpr::LowercaseFirstChar {
                            parameter,
                            indirect,
                            pattern,
                        },
                    })
                }) {
                    return Ok(e);
                }
            }
        }
        form(input, &|_, indirect, parameter| {
            Ok(ParameterExpr::Parameter {
                parameter,
                indirect,
            })
        })
        .ok_or(Fail)
    }

    /// `!`, when indirection is allowed, then a parameter.
    fn indirect_parameter(&self, input: &mut &'a str) -> Res<(bool, Parameter)> {
        let indirect = !self.options.sh_mode && !input.is_empty() && input.starts_with('!');
        if indirect {
            *input = &input[1..];
        }
        Ok((indirect, self.parameter(input)?))
    }

    /// The text a word up to `stop` takes, as written.
    fn text_of_word(&self, input: &mut &'a str, stop: Stop) -> String {
        let start = *input;
        let _ = self.word(input, stop);
        start[..start.len() - input.len()].to_owned()
    }

    /// The text of an arithmetic word up to `stop`, as written.
    fn arithmetic_text(&self, input: &mut &'a str, stop: Stop) -> ast::UnexpandedArithmeticExpr {
        let start = *input;
        self.arithmetic_word(input, stop);
        ast::UnexpandedArithmeticExpr {
            value: start[..start.len() - input.len()].to_owned(),
        }
    }

    pub(super) fn parameter(&self, input: &mut &'a str) -> Res<Parameter> {
        let bytes = input.as_bytes();
        let first = *bytes.first().ok_or(Fail)?;
        if (b'1'..=b'9').contains(&first) {
            let len = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            let n = input[..len].parse().map_err(|_| Fail)?;
            *input = &input[len..];
            return Ok(Parameter::Positional(n));
        }
        if let Some(special) = special_parameter(first) {
            *input = &input[1..];
            return Ok(Parameter::Special(special));
        }
        let name = variable_name(input)?;
        if !self.options.sh_mode {
            if let Some(rest) = input.strip_prefix("[@]") {
                *input = rest;
                return Ok(Parameter::NamedWithAllIndices {
                    name: name.to_owned(),
                    concatenate: false,
                });
            }
            if let Some(rest) = input.strip_prefix("[*]") {
                *input = rest;
                return Ok(Parameter::NamedWithAllIndices {
                    name: name.to_owned(),
                    concatenate: true,
                });
            }
            if let Ok(index) = attempt(input, |i| {
                literal(i, "[")?;
                let start = *i;
                self.arithmetic_word(i, Stop::Bracket);
                let index = &start[..start.len() - i.len()];
                literal(i, "]")?;
                Ok(index)
            }) {
                return Ok(Parameter::NamedWithIndex {
                    name: name.to_owned(),
                    index: index.to_owned(),
                });
            }
        }
        Ok(Parameter::Named(name.to_owned()))
    }

    fn unbraced_parameter(&self, input: &mut &'a str) -> Res<Parameter> {
        let _ = self;
        let first = *input.as_bytes().first().ok_or(Fail)?;
        if (b'1'..=b'9').contains(&first) {
            *input = &input[1..];
            return Ok(Parameter::Positional(u32::from(first - b'0')));
        }
        if let Some(special) = special_parameter(first) {
            *input = &input[1..];
            return Ok(Parameter::Special(special));
        }
        Ok(Parameter::Named(variable_name(input)?.to_owned()))
    }

    // ---- Arithmetic words ---------------------------------------------------------------

    /// Pieces of an arithmetic word until `stop`, parentheses matched.
    fn arithmetic_word(&self, input: &mut &'a str, stop: Stop) {
        while attempt(input, |i| self.arithmetic_word_piece(i, stop)).is_ok() {}
    }

    fn arithmetic_word_piece(&self, input: &mut &'a str, stop: Stop) -> Res<()> {
        if input.starts_with('(') {
            *input = &input[1..];
            self.arithmetic_word(input, Stop::Paren);
            return literal(input, ")");
        }
        if attempt(input, |i| self.array_element_name(i)).is_ok() {
            return Ok(());
        }
        self.word_piece(input, stop.or_open_paren()).map(|_| ())
    }

    fn array_element_name(&self, input: &mut &'a str) -> Res<(&'a str, &'a str)> {
        let name = assigned_scalar_name(input)?;
        literal(input, "[")?;
        let start = *input;
        self.arithmetic_word(input, Stop::Bracket);
        let index = &start[..start.len() - input.len()];
        literal(input, "]")?;
        Ok((name, index))
    }

    // ---- Tildes -------------------------------------------------------------------------

    fn tilde_expression_piece(&self, input: &mut &'a str) -> Res<WordPiece> {
        let _ = self;
        literal(input, "~")?;
        Ok(WordPiece::TildeExpansion(tilde_expression(input)?))
    }

    /// Whether a tilde expression after a colon starts at `rest`, inside the word.
    fn tilde_after_colon_here(&self, rest: &'a str) -> bool {
        self.options.tilde_expansion_after_colon
            && rest.starts_with('~')
            && self.char_before(rest) == Some(b':')
            && tilde_expression(&mut &rest[1..]).is_ok()
    }

    fn tilde_expr_after_colon(&self, input: &mut &'a str) -> Res<WordPiece> {
        if !self.tilde_after_colon_here(input) {
            return Err(Fail);
        }
        self.tilde_expression_piece(input)
    }

    fn char_before(&self, rest: &str) -> Option<u8> {
        let at = self.offset(rest);
        at.checked_sub(1).map(|i| self.word.as_bytes()[i])
    }

    // ---- Here-documents -----------------------------------------------------------------

    fn heredoc_word(&self, input: &mut &'a str) -> Vec<WordPieceWithSource> {
        let mut pieces = Vec::new();
        while !input.is_empty() {
            let start = self.offset(input);
            let Ok(piece) = self.heredoc_word_piece(input) else {
                break;
            };
            pieces.push(WordPieceWithSource {
                piece,
                start_index: start,
                end_index: self.offset(input),
            });
        }
        pieces
    }

    fn heredoc_word_piece(&self, input: &mut &'a str) -> Res<WordPiece> {
        if input.starts_with('$') {
            if let Ok(piece) = attempt(input, |i| self.dollar_sign_word_piece(i)) {
                return Ok(piece);
            }
        }
        if input.starts_with('`') {
            if let Ok(piece) = attempt(input, |i| self.backquoted(i)) {
                return Ok(piece);
            }
        }
        if let Ok(piece) = attempt(input, |i| escape_in(i, b"$`\\")) {
            return Ok(piece);
        }
        let start = *input;
        loop {
            let rest = *input;
            let Some(&b) = rest.as_bytes().first() else {
                break;
            };
            if b == b'`' {
                break;
            }
            if b == b'\\' && rest.as_bytes().get(1).is_some_and(|c| b"$`\\".contains(c)) {
                break;
            }
            if b == b'$' && (self.starts_dollar_piece(rest) || is_expansion_opener(rest)) {
                break;
            }
            let _ = next_char(input);
        }
        let len = start.len() - input.len();
        if len == 0 {
            return Err(Fail);
        }
        Ok(WordPiece::Text(start[..len].to_owned()))
    }

    // ---- Brace expansion ----------------------------------------------------------------

    fn brace_expansion_piece(&self, input: &mut &'a str, stop: Stop) -> Res<BraceExpressionOrText> {
        if let Ok(expr) = attempt(input, |i| self.brace_expr(i)) {
            return Ok(BraceExpressionOrText::Expr(expr));
        }
        let start = *input;
        while attempt(input, |i| self.non_brace_expr_text(i, stop)).is_ok() {}
        let len = start.len() - input.len();
        if len == 0 {
            return Err(Fail);
        }
        Ok(BraceExpressionOrText::Text(start[..len].to_owned()))
    }

    fn non_brace_expr_text(&self, input: &mut &'a str, stop: Stop) -> Res<()> {
        if !input.starts_with('{') {
            let open_brace: &'static Stop = match stop {
                Stop::End => &Stop::End,
                Stop::CommaOrBrace => &Stop::CommaOrBrace,
                _ => &Stop::End,
            };
            return self
                .word_piece(input, Stop::OpenBrace(open_brace))
                .map(|_| ());
        }
        if attempt(&mut { *input }, |i| self.brace_expr(i)).is_ok() || stop.at(input) {
            return Err(Fail);
        }
        *input = &input[1..];
        Ok(())
    }

    fn brace_expr(&self, input: &mut &'a str) -> Res<BraceExpression> {
        literal(input, "{")?;
        let inner = attempt(input, |i| self.brace_text_list_expr(i))
            .or_else(|_| attempt(input, |i| brace_sequence_expr(i).map(|s| vec![s])))?;
        literal(input, "}")?;
        Ok(inner)
    }

    fn brace_text_list_expr(&self, input: &mut &'a str) -> Res<BraceExpression> {
        let mut members = vec![self.brace_text_list_member(input)?];
        while let Ok(member) = attempt(input, |i| {
            literal(i, ",")?;
            self.brace_text_list_member(i)
        }) {
            members.push(member);
        }
        if members.len() < 2 {
            return Err(Fail);
        }
        Ok(members)
    }

    fn brace_text_list_member(&self, input: &mut &'a str) -> Res<BraceExpressionMember> {
        if input.starts_with([',', '}']) {
            return Ok(BraceExpressionMember::Child(vec![
                BraceExpressionOrText::Text(String::new()),
            ]));
        }
        let mut pieces = Vec::new();
        while let Ok(piece) = attempt(input, |i| self.brace_expansion_piece(i, Stop::CommaOrBrace))
        {
            pieces.push(piece);
        }
        if pieces.is_empty() {
            return Err(Fail);
        }
        Ok(BraceExpressionMember::Child(pieces))
    }

    // ---- Assignments --------------------------------------------------------------------

    fn assignment_name(&self, input: &mut &'a str) -> Res<ast::AssignmentName> {
        if let Ok((name, index)) = attempt(input, |i| self.array_element_name(i)) {
            return Ok(ast::AssignmentName::ArrayElementName(
                name.to_owned(),
                index.to_owned(),
            ));
        }
        Ok(ast::AssignmentName::VariableName(
            assigned_scalar_name(input)?.to_owned(),
        ))
    }

    fn name_equals(&self, input: &mut &'a str) -> Res<(ast::AssignmentName, bool)> {
        let name = self.assignment_name(input)?;
        let append = input.starts_with('+');
        if append {
            *input = &input[1..];
        }
        literal(input, "=")?;
        Ok((name, append))
    }
}

// ---- Pieces that need no state ---------------------------------------------------------

fn single_quoted_literal_text<'a>(input: &mut &'a str) -> Res<&'a str> {
    literal(input, "'")?;
    let end = input.find('\'').ok_or(Fail)?;
    let inner = &input[..end];
    *input = &input[end + 1..];
    Ok(inner)
}

/// `$'…'`: a backslash escapes the character after it.
fn ansi_c_quoted_text<'a>(input: &mut &'a str) -> Res<&'a str> {
    literal(input, "$'")?;
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if matches!(bytes.get(i + 1), Some(b'\\' | b'\'')) => i += 2,
            b'\'' => {
                let inner = &input[..i];
                *input = &input[i + 1..];
                return Ok(inner);
            }
            _ => i += 1,
        }
    }
    Err(Fail)
}

fn normal_escape_sequence(input: &mut &str) -> Res<WordPiece> {
    let rest = *input;
    literal(input, "\\")?;
    let c = next_char(input)?;
    Ok(WordPiece::EscapeSequence(rest[..=c.len_utf8()].to_owned()))
}

/// A backslash and one of `chars` after it.
fn escape_in(input: &mut &str, chars: &[u8]) -> Res<WordPiece> {
    let bytes = input.as_bytes();
    if bytes.first() == Some(&b'\\') && bytes.get(1).is_some_and(|c| chars.contains(c)) {
        let piece = WordPiece::EscapeSequence(input[..2].to_owned());
        *input = &input[2..];
        return Ok(piece);
    }
    Err(Fail)
}

/// `$(` or `$[`: as in bash, these always start an expansion.
fn is_expansion_opener(rest: &str) -> bool {
    rest.starts_with("$(") || rest.starts_with("$[")
}

const fn special_parameter(b: u8) -> Option<SpecialParameter> {
    Some(match b {
        b'@' => SpecialParameter::AllPositionalParameters { concatenate: false },
        b'*' => SpecialParameter::AllPositionalParameters { concatenate: true },
        b'#' => SpecialParameter::PositionalParameterCount,
        b'?' => SpecialParameter::LastExitStatus,
        b'-' => SpecialParameter::CurrentOptionFlags,
        b'$' => SpecialParameter::ProcessId,
        b'!' => SpecialParameter::LastBackgroundProcessId,
        b'0' => SpecialParameter::ShellName,
        _ => return None,
    })
}

/// A name that does not start with a digit.
fn variable_name<'a>(input: &mut &'a str) -> Res<&'a str> {
    let bytes = input.as_bytes();
    if bytes.first().is_none_or(u8::is_ascii_digit) {
        return Err(Fail);
    }
    let len = bytes.iter().take_while(|&&b| is_name_char(b)).count();
    if len == 0 {
        return Err(Fail);
    }
    let name = &input[..len];
    *input = &input[len..];
    Ok(name)
}

fn assigned_scalar_name<'a>(input: &mut &'a str) -> Res<&'a str> {
    let bytes = input.as_bytes();
    if !bytes
        .first()
        .is_some_and(|&b| b == b'_' || b.is_ascii_alphabetic())
    {
        return Err(Fail);
    }
    let len = bytes.iter().take_while(|&&b| is_name_char(b)).count();
    let name = &input[..len];
    *input = &input[len..];
    Ok(name)
}

fn tilde_terminator(rest: &str) -> bool {
    rest.is_empty() || rest.starts_with(['/', ':', ';', '}'])
}

fn tilde_expression(input: &mut &str) -> Res<TildeExpr> {
    if tilde_terminator(input) {
        return Ok(TildeExpr::Home);
    }
    if let Some(rest) = input.strip_prefix('+')
        && tilde_terminator(rest)
    {
        *input = rest;
        return Ok(TildeExpr::WorkingDir);
    }
    {
        let plus = input.starts_with('+');
        let rest = if plus { &input[1..] } else { *input };
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if tilde_terminator(&rest[digits..])
            && let Ok(n) = rest[..digits].parse()
        {
            *input = &rest[digits..];
            return Ok(TildeExpr::NthDirFromTopOfDirStack { n, plus_used: plus });
        }
    }
    if let Some(rest) = input.strip_prefix('-') {
        if tilde_terminator(rest) {
            *input = rest;
            return Ok(TildeExpr::OldWorkingDir);
        }
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if tilde_terminator(&rest[digits..])
            && let Ok(n) = rest[..digits].parse()
        {
            *input = &rest[digits..];
            return Ok(TildeExpr::NthDirFromBottomOfDirStack { n });
        }
    }
    let len = input
        .bytes()
        .take_while(|&b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        .count();
    if tilde_terminator(&input[len..]) {
        let user = input[..len].to_owned();
        *input = &input[len..];
        return Ok(TildeExpr::UserHome(user));
    }
    Err(Fail)
}

fn brace_sequence_expr(input: &mut &str) -> Res<BraceExpressionMember> {
    if let Ok(member) = attempt(input, |i| {
        let start = sequence_bound(i)?;
        literal(i, "..")?;
        let end = sequence_bound(i)?;
        let increment = attempt(i, |i| {
            literal(i, "..")?;
            number(i)
        })
        .ok();
        Ok(BraceExpressionMember::NumberSequence {
            start: start.0,
            end: end.0,
            increment: increment.unwrap_or(1),
            zero_padded_width: zero_padded_width(start.1, end.1),
        })
    }) {
        return Ok(member);
    }
    let start = next_char(input)
        .ok()
        .filter(char::is_ascii_alphabetic)
        .ok_or(Fail)?;
    literal(input, "..")?;
    let end = next_char(input)
        .ok()
        .filter(char::is_ascii_alphabetic)
        .ok_or(Fail)?;
    let increment = attempt(input, |i| {
        literal(i, "..")?;
        number(i)
    })
    .ok();
    Ok(BraceExpressionMember::CharSequence {
        start,
        end,
        increment: increment.unwrap_or(1),
    })
}

fn sequence_bound<'a>(input: &mut &'a str) -> Res<(i64, &'a str)> {
    let bytes = input.as_bytes();
    let sign = usize::from(bytes.first().is_some_and(|b| matches!(b, b'-' | b'+')));
    let digits = bytes[sign..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();
    if digits == 0 {
        return Err(Fail);
    }
    let text = &input[..sign + digits];
    let n = parse_sequence_bound(text).ok_or(Fail)?;
    *input = &input[sign + digits..];
    Ok((n, text))
}

fn number(input: &mut &str) -> Res<i64> {
    let bytes = input.as_bytes();
    let (sign, skip) = match bytes.first() {
        Some(b'-') => (-1, 1),
        Some(b'+') => (1, 1),
        _ => (1, 0),
    };
    let digits = bytes[skip..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();
    if digits == 0 {
        return Err(Fail);
    }
    let n: i64 = input[skip..skip + digits].parse().map_err(|_| Fail)?;
    *input = &input[skip + digits..];
    Ok(n * sign)
}

// ---- Entry points ------------------------------------------------------------------------

/// A word's pieces; the whole word must be read.
pub(super) fn parse(word: &str, options: &ParserOptions) -> Option<Vec<WordPieceWithSource>> {
    let grammar = Grammar {
        word,
        options,
        last_dollar: std::cell::RefCell::new(None),
    };
    let mut input = word;
    let pieces = grammar.word(&mut input, Stop::End);
    input.is_empty().then_some(pieces)
}

/// A here-document body's pieces.
pub(super) fn parse_heredoc(
    word: &str,
    options: &ParserOptions,
) -> Option<Vec<WordPieceWithSource>> {
    let grammar = Grammar {
        word,
        options,
        last_dollar: std::cell::RefCell::new(None),
    };
    let mut input = word;
    let pieces = grammar.heredoc_word(&mut input);
    input.is_empty().then_some(pieces)
}

/// A parameter; the whole word must be one.
pub(super) fn parse_parameter(word: &str, options: &ParserOptions) -> Option<Parameter> {
    let grammar = Grammar {
        word,
        options,
        last_dollar: std::cell::RefCell::new(None),
    };
    let mut input = word;
    let parameter = grammar.parameter(&mut input).ok()?;
    input.is_empty().then_some(parameter)
}

/// A word cut into brace expressions and text, as the PEG grammar's `brace_expansions`
/// returns it: `Ok(None)` when it holds no piece at all, and `Err` when the pieces do not
/// take the whole word.
pub(super) fn parse_brace_expansions(
    word: &str,
    options: &ParserOptions,
) -> Result<Option<Vec<BraceExpressionOrText>>, ()> {
    let grammar = Grammar {
        word,
        options,
        last_dollar: std::cell::RefCell::new(None),
    };
    let mut input = word;
    let mut pieces = Vec::new();
    while let Ok(piece) = attempt(&mut input, |i| grammar.brace_expansion_piece(i, Stop::End)) {
        pieces.push(piece);
    }
    if pieces.is_empty() {
        return Ok(None);
    }
    // The PEG rule commits to its first alternative once a piece matched.
    if input.is_empty() {
        Ok(Some(pieces))
    } else {
        Err(())
    }
}

/// `name=value` or `name+=value`.
pub(super) fn parse_scalar_assignment(
    word: &str,
    options: &ParserOptions,
) -> Option<ast::Assignment> {
    let grammar = Grammar {
        word,
        options,
        last_dollar: std::cell::RefCell::new(None),
    };
    let mut input = word;
    let (name, append) = grammar.name_equals(&mut input).ok()?;
    Some(ast::Assignment {
        name,
        value: ast::AssignmentValue::Scalar(ast::Word::from(input.to_owned())),
        append,
        loc: crate::SourceSpan::default(),
    })
}

/// `name=` or `name+=`, the whole word.
pub(super) fn parse_name_equals(
    word: &str,
    options: &ParserOptions,
) -> Option<(ast::AssignmentName, bool)> {
    let grammar = Grammar {
        word,
        options,
        last_dollar: std::cell::RefCell::new(None),
    };
    let mut input = word;
    let result = grammar.name_equals(&mut input).ok()?;
    input.is_empty().then_some(result)
}

/// `[key]=value` or `value`.
pub(super) fn parse_literal_array_element(word: &str) -> Option<(Option<String>, String)> {
    if let Some(rest) = word.strip_prefix('[')
        && let Some(end) = rest.find(']')
        && let Some(value) = rest[end..].strip_prefix("]=")
    {
        return Some((Some(rest[..end].to_owned()), value.to_owned()));
    }
    (!word.is_empty()).then(|| (None, word.to_owned()))
}
