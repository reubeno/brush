//! Idiomatic winnow parser implementation - parsers return `impl Parser`
//!
//! This module contains refactored parser functions that follow winnow best practices:
//! - Parsers return `impl Parser<Input, Output, Error>` instead of taking input directly
//! - Use combinators like `alt`, `repeat`, `separated` instead of manual loops
//! - Use `Stateful` stream to thread parser context through the parser tree
//! - Declarative, composable, and easier to reason about

#![allow(dead_code)]

use winnow::combinator::{alt, cut_err, dispatch, fail, repeat};
use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::stream::Stateful;
use winnow::token::take;

use crate::ast::SeparatorOperator;
use crate::parser::{ParserOptions, SourceInfo};
use crate::tokenizer::Token;

/// Type alias for parser error
type PError = winnow::error::ErrMode<ContextError>;

// ============================================================================
// Stream Types with Context Support
// ============================================================================

/// Parser context carrying options and source info
#[derive(Clone, Copy)]
pub struct ParserContext<'a> {
    /// Parser options controlling parsing behavior
    pub options: &'a ParserOptions,
    /// Source information for error reporting
    pub source_info: &'a SourceInfo,
}

impl std::fmt::Debug for ParserContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParserContext")
            .field("options", &"<ParserOptions>")
            .field("source_info", self.source_info)
            .finish()
    }
}

/// Token stream without context - for basic parsers that don't need options
pub type TokenStream<'a> = &'a [Token];

/// Token stream with parser context - for parsers that need options/source info
/// This uses winnow's Stateful wrapper to thread context through the parser tree
pub type StatefulStream<'a> = Stateful<&'a [Token], ParserContext<'a>>;

/// Create a stateful stream from tokens and context
pub fn make_stream<'a>(tokens: &'a [Token], ctx: ParserContext<'a>) -> StatefulStream<'a> {
    Stateful {
        input: tokens,
        state: ctx,
    }
}

// ============================================================================
// Tier 0: Pure Token Parsers
// ============================================================================

/// Parse any word token
pub fn word<'a>() -> impl Parser<&'a [Token], &'a Token, PError> {
    take(1usize).verify_map(|slice: &[Token]| match &slice[0] {
        token @ Token::Word(_, _) => Some(token),
        _ => None,
    })
}

/// Match specific operator
pub fn matches_operator<'a>(op: &'a str) -> impl Parser<&'a [Token], &'a Token, PError> + 'a {
    take(1usize).verify_map(move |slice: &[Token]| match &slice[0] {
        token @ Token::Operator(s, _) if s == op => Some(token),
        _ => None,
    })
}

/// Match specific word
pub fn matches_word<'a>(w: &'a str) -> impl Parser<&'a [Token], &'a Token, PError> + 'a {
    take(1usize).verify_map(move |slice: &[Token]| match &slice[0] {
        token @ Token::Word(s, _) if s == w => Some(token),
        _ => None,
    })
}

/// Peek at operator string for dispatch (doesn't consume)
fn peek_operator<'a>() -> impl Parser<&'a [Token], &'a str, PError> {
    winnow::combinator::peek(take(1usize)).verify_map(|slice: &[Token]| match &slice[0] {
        Token::Operator(_, _) => Some(slice[0].to_str()),
        _ => None,
    })
}

// ============================================================================
// Tier 1: Basic Combinators (separators, linebreaks, operators)
// ============================================================================

/// Parse linebreak (zero or more newlines)
/// Equivalent to: `newline()* {}`
pub fn linebreak<'a>() -> impl Parser<&'a [Token], (), PError> {
    // Use () accumulator to avoid Vec allocation
    repeat::<_, _, (), _, _>(0.., matches_operator("\n").void())
}

/// Parse newline list (one or more newlines)
/// Equivalent to: `newline()+ {}`
pub fn newline_list<'a>() -> impl Parser<&'a [Token], (), PError> {
    // Use () accumulator to avoid Vec allocation
    repeat::<_, _, (), _, _>(1.., matches_operator("\n").void())
}

/// Helper function to map an operator to a value
/// This is a convenience wrapper around matches_operator(op).value(value)
fn op_value<'a, T>(op: &'a str, value: T) -> impl Parser<&'a [Token], T, PError>
where
    T: Clone,
{
    matches_operator(op).value(value)
}

/// Helper function to create a backtrack error
/// This is a convenience wrapper for creating consistent backtrack errors
fn backtrack_error<T>() -> Result<T, PError> {
    use winnow::error::ErrMode;
    Err(ErrMode::Backtrack(ContextError::default()))
}

/// Parse separator operator (; or &)
/// Returns the separator type
pub fn separator_op<'a>() -> impl Parser<&'a [Token], SeparatorOperator, PError> {
    dispatch! {peek_operator();
        ";" => op_value(";", SeparatorOperator::Sequence),
        "&" => op_value("&", SeparatorOperator::Async),
        _ => fail,
    }
}

/// Parse separator (separator_op with linebreak, or newline_list)
/// Returns Option<SeparatorOperator> - None means it was just newlines
pub fn separator<'a>() -> impl Parser<&'a [Token], Option<SeparatorOperator>, PError> {
    alt((
        (separator_op(), linebreak()).map(|(s, _)| Some(s)),
        newline_list().map(|_| None),
    ))
}

/// Parse sequential separator (; followed by linebreak, or newline_list)
pub fn sequential_sep<'a>() -> impl Parser<&'a [Token], (), PError> {
    alt((
        (matches_operator(";"), linebreak()).map(|_| ()),
        newline_list().map(|_| ()),
    ))
}

/// Parse AND/OR operator (&& or ||)
/// Returns a function that wraps a Pipeline into an AndOr variant
pub fn and_or_op<'a>()
-> impl Parser<&'a [Token], fn(crate::ast::Pipeline) -> crate::ast::AndOr, PError> {
    dispatch! {peek_operator();
        "&&" => op_value("&&", crate::ast::AndOr::And as fn(_) -> _),
        "||" => op_value("||", crate::ast::AndOr::Or as fn(_) -> _),
        _ => fail,
    }
}

/// Parse pipe operator (| or |&)
/// Returns true if it's |& (which requires adding stderr redirect)
pub fn pipe_operator<'a>() -> impl Parser<&'a [Token], bool, PError> {
    dispatch! {peek_operator();
        "|&" => op_value("|&", true),
        "|" => op_value("|", false),
        _ => fail,
    }
}

// ============================================================================
// Tier 2: Word-related Parsers
// ============================================================================

/// Check if string is a reserved word in shell grammar
/// Optimized with match for O(1) lookup - compiler generates perfect hash/jump table
#[inline]
fn is_reserved_word(s: &str) -> bool {
    matches!(
        s,
        "if" | "then"
            | "else"
            | "elif"
            | "fi"
            | "case"
            | "esac"
            | "for"
            | "select"
            | "while"
            | "until"
            | "do"
            | "done"
            | "in"
            | "function"
            | "{"
            | "}"
            | "[["
            | "]]"
            | "!"
            | "time"
    )
}

/// Check if string is a valid variable name
fn is_valid_name(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    let mut chars = s.chars();
    let first = chars.next().unwrap();

    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Parse a word token and convert it to AST Word
/// This is a common pattern used throughout the parser
pub fn word_as_ast<'a>() -> impl Parser<&'a [Token], crate::ast::Word, PError> {
    word().map(crate::ast::Word::from)
}

/// Parse wordlist (list of words) - one or more words
/// Equivalent to PEG: `(w:word() { ast::Word::from(w) })+`
pub fn wordlist<'a>() -> impl Parser<&'a [Token], Vec<crate::ast::Word>, PError> {
    repeat(1.., word_as_ast())
}

/// Parse a variable name (valid identifier)
pub fn name<'a>() -> impl Parser<&'a [Token], &'a str, PError> {
    word()
        .verify(|tok: &&Token| is_valid_name(tok.to_str()))
        .map(|tok: &Token| tok.to_str())
}

/// Parse non-reserved word
pub fn non_reserved_word<'a>() -> impl Parser<&'a [Token], &'a Token, PError> {
    word().verify(|tok: &&Token| !is_reserved_word(tok.to_str()))
}

// ============================================================================
// Tier 2: I/O Redirect Parsers
// ============================================================================

/// Check if a token is a redirect operator
#[inline]
fn is_redirect_operator(op: &str) -> bool {
    matches!(
        op,
        "<" | ">" | ">>" | "<<" | "<<-" | "<&" | ">&" | "&>" | "<>" | ">|"
    )
}

/// Check if token slice looks like an I/O redirect (used for lookahead optimization)
/// Returns true if:
/// - First token is a redirect operator
/// - First token is a number and second token is a redirect operator
#[inline]
fn looks_like_io_redirect(tokens: &[Token]) -> bool {
    match tokens.first() {
        Some(Token::Operator(op, _)) => is_redirect_operator(op.as_str()),
        Some(Token::Word(w, _)) if w.chars().all(|c| c.is_ascii_digit()) => {
            // fd number followed by redirect operator
            matches!(tokens.get(1), Some(Token::Operator(op, _))
                if op.starts_with('<') || op.starts_with('>'))
        }
        _ => false,
    }
}

/// Parse I/O number (file descriptor number)
/// Must be followed by a redirect operator (< or >)
pub fn io_number<'a>() -> impl Parser<&'a [Token], i32, PError> {
    word().verify_map(|tok: &Token| {
        let s = tok.to_str();
        if s.chars().all(|c| c.is_ascii_digit()) {
            s.parse::<i32>().ok()
        } else {
            None
        }
    })
}

/// Parse filename for redirect target
pub fn io_filename<'a>() -> impl Parser<&'a [Token], crate::ast::IoFileRedirectTarget, PError> {
    word_as_ast().map(crate::ast::IoFileRedirectTarget::Filename)
}

/// Parse FD duplication source (for >&N or <&N)
pub fn io_fd_duplication_source<'a>()
-> impl Parser<&'a [Token], crate::ast::IoFileRedirectTarget, PError> {
    word_as_ast().map(crate::ast::IoFileRedirectTarget::Duplicate)
}

/// Parse file redirect operator and target
/// Returns (kind, target)
pub fn io_file<'a>() -> impl Parser<
    &'a [Token],
    (
        crate::ast::IoFileRedirectKind,
        crate::ast::IoFileRedirectTarget,
    ),
    PError,
> {
    dispatch! {peek_operator();
        ">&" => io_file_inner(">&", io_fd_duplication_source(), || crate::ast::IoFileRedirectKind::DuplicateOutput),
        "<&" => io_file_inner("<&", io_fd_duplication_source(), || crate::ast::IoFileRedirectKind::DuplicateInput),
        ">>" => io_file_inner(">>", io_filename(), || crate::ast::IoFileRedirectKind::Append),
        "<>" => io_file_inner("<>", io_filename(), || crate::ast::IoFileRedirectKind::ReadAndWrite),
        ">|" => io_file_inner(">|", io_filename(), || crate::ast::IoFileRedirectKind::Clobber),
        "<" => io_file_inner("<", io_filename(), || crate::ast::IoFileRedirectKind::Read),
        ">" => io_file_inner(">", io_filename(), || crate::ast::IoFileRedirectKind::Write),
        _ => fail,
    }
}

/// Parse redirect list (one or more redirects)
pub fn redirect_list<'a>() -> impl Parser<&'a [Token], crate::ast::RedirectList, PError> {
    repeat(1.., io_redirect()).map(crate::ast::RedirectList)
}

/// Parse a single I/O redirect (file redirect or heredoc)
pub fn io_redirect<'a>() -> impl Parser<&'a [Token], crate::ast::IoRedirect, PError> {
    // Optimization: Factor out opt(io_number()) to avoid parsing it twice
    // First parse optional fd number, then dispatch based on operator type
    |input: &mut &'a [Token]| {
        let fd = winnow::combinator::opt(io_number()).parse_next(input)?;

        // Peek at operator to determine if it's heredoc or file redirect
        let is_heredoc = matches!(peek_operator().parse_peek(*input), Ok((_, "<<" | "<<-")));

        if is_heredoc {
            let doc = io_here().parse_next(input)?;
            Ok(crate::ast::IoRedirect::HereDocument(fd, doc))
        } else {
            let (kind, target) = io_file().parse_next(input)?;
            Ok(crate::ast::IoRedirect::File(fd, kind, target))
        }
    }
}

/// Inner function for file redirect parsing that handles the complete parsing pipeline
/// Takes the operator, target parser, and a function that provides the redirect kind, returns a
/// parser
fn io_file_inner<'a, F, T, K>(
    op: &'a str,
    target_parser: F,
    kind_fn: K,
) -> impl Parser<&'a [Token], (crate::ast::IoFileRedirectKind, T), PError>
where
    F: Parser<&'a [Token], T, PError> + 'a,
    K: Fn() -> crate::ast::IoFileRedirectKind + 'a,
{
    (matches_operator(op), cut_err(target_parser)).map(move |(_, t)| (kind_fn(), t))
}

/// Inner function for heredoc parsing that handles the complete parsing pipeline
/// Takes the heredoc operator and remove_tabs flag, returns a parser
fn io_here_inner<'a>(
    op: &'a str,
    remove_tabs: bool,
) -> impl Parser<&'a [Token], crate::ast::IoHereDocument, PError> {
    (matches_operator(op), cut_err((word(), word(), word()))).verify_map(
        move |(_, (tag, doc, closing))| {
            if tag.to_str() == closing.to_str() {
                let tag_str = tag.to_str();
                let requires_expansion = !tag_str.contains(['\'', '"', '\\']);
                Some(crate::ast::IoHereDocument {
                    remove_tabs,
                    requires_expansion,
                    here_end: crate::ast::Word::from(tag),
                    doc: crate::ast::Word::from(doc),
                })
            } else {
                None
            }
        },
    )
}

/// Parse heredoc operator and content
/// Note: This is a simplified version - full heredoc requires tokenizer support
pub fn io_here<'a>() -> impl Parser<&'a [Token], crate::ast::IoHereDocument, PError> {
    dispatch! {peek_operator();
        "<<-" => io_here_inner("<<-", true),
        "<<" => io_here_inner("<<", false),
        _ => fail,
    }
}

// ============================================================================
// Tier 2: Assignment Parsers
// ============================================================================

/// Result of parsing an assignment - contains both the Assignment AST and the original Word
pub type AssignmentResult = (crate::ast::Assignment, crate::ast::Word);

/// Parse a scalar assignment from a word token (VAR=value or VAR+=value or VAR[idx]=value)
/// Returns None if the word is not a valid assignment
fn parse_scalar_assignment(tok: &Token) -> Option<AssignmentResult> {
    let s = tok.to_str();
    let loc = tok.location();

    // Split at '=' to get name and value parts
    let (before_eq, value_str) = s.split_once('=')?;

    // Check for += (append)
    let (var_part, append) = match before_eq.strip_suffix('+') {
        Some(name) if !name.is_empty() => (name, true),
        _ => (before_eq, false),
    };

    // Check for array element assignment VAR[index]=value
    if let Some(bracket_pos) = var_part.find('[') {
        if let Some(with_bracket) = var_part.strip_suffix(']') {
            let var_name = &with_bracket[..bracket_pos];
            let index = &with_bracket[bracket_pos + 1..];

            if is_valid_name(var_name) {
                return Some((
                    crate::ast::Assignment {
                        name: crate::ast::AssignmentName::ArrayElementName(
                            var_name.to_string(),
                            index.to_string(),
                        ),
                        value: crate::ast::AssignmentValue::Scalar(crate::ast::Word::from(
                            value_str.to_string(),
                        )),
                        append,
                        loc: loc.clone(),
                    },
                    crate::ast::Word::with_location(s, loc),
                ));
            }
        }
    }

    // Simple variable assignment
    if is_valid_name(var_part) {
        return Some((
            crate::ast::Assignment {
                name: crate::ast::AssignmentName::VariableName(var_part.to_string()),
                value: crate::ast::AssignmentValue::Scalar(crate::ast::Word::from(
                    value_str.to_string(),
                )),
                append,
                loc: loc.clone(),
            },
            crate::ast::Word::with_location(s, loc),
        ));
    }

    None
}

/// Parse scalar assignment word (VAR=value, VAR+=value, VAR[idx]=value)
pub fn scalar_assignment_word<'a>() -> impl Parser<&'a [Token], AssignmentResult, PError> {
    word().verify_map(parse_scalar_assignment)
}

/// Parse array elements inside parentheses (for array assignment)
/// Parses: elem1 elem2 elem3 ...
fn array_elements<'a>()
-> impl Parser<&'a [Token], Vec<(Option<crate::ast::Word>, crate::ast::Word)>, PError> {
    repeat(0.., word_as_ast().map(|word| (None, word)))
}

/// Parse array assignment: VAR=(elem1 elem2 ...)
/// The word token should end with '=' and be followed by '(' elements ')'
pub fn array_assignment_word<'a>() -> impl Parser<&'a [Token], AssignmentResult, PError> {
    (
        // Word ending with '=' that has a valid variable name before it
        word().verify(|tok: &&Token| {
            tok.to_str()
                .strip_suffix('=')
                .map_or(false, |name| is_valid_name(name))
        }),
        matches_operator("("),
        array_elements(),
        matches_operator(")"),
    )
        .map(|(name_tok, _, elements, end_tok)| {
            let s = name_tok.to_str();
            let var_name = &s[..s.len() - 1];
            let start_loc = name_tok.location();
            let end_loc = end_tok.location();

            // Build the full word representation
            let mut all_as_word = s.to_string();
            all_as_word.push('(');
            for (i, (_, elem)) in elements.iter().enumerate() {
                if i > 0 {
                    all_as_word.push(' ');
                }
                all_as_word.push_str(&elem.value);
            }
            all_as_word.push(')');

            let full_loc = crate::SourceSpan {
                start: start_loc.start.clone(),
                end: end_loc.end.clone(),
            };

            (
                crate::ast::Assignment {
                    name: crate::ast::AssignmentName::VariableName(var_name.to_string()),
                    value: crate::ast::AssignmentValue::Array(elements),
                    append: false,
                    loc: full_loc.clone(),
                },
                crate::ast::Word::with_location(&all_as_word, &full_loc),
            )
        })
}

/// Parse assignment word - either array assignment or scalar assignment
/// Array assignment: VAR=(elem1 elem2 ...)
/// Scalar assignment: VAR=value, VAR+=value, VAR[idx]=value
pub fn assignment_word<'a>() -> impl Parser<&'a [Token], AssignmentResult, PError> {
    // Use lookahead to determine which to try first
    // If we see VAR= followed by '(', try array first (to avoid scalar consuming VAR= with empty
    // value) Otherwise try scalar first (99% of cases)
    |input: &mut &'a [Token]| {
        let try_array_first = if let Some(first) = input.first() {
            if let Token::Word(w, _) = first {
                // Check if word ends with '=' and is followed by '('
                w.ends_with('=')
                    && matches!(input.get(1), Some(Token::Operator(op, _)) if op == "(")
            } else {
                false
            }
        } else {
            false
        };

        if try_array_first {
            // Try array first when we detect VAR= followed by (
            alt((array_assignment_word(), scalar_assignment_word())).parse_next(input)
        } else {
            // Try scalar first (most common case)
            alt((scalar_assignment_word(), array_assignment_word())).parse_next(input)
        }
    }
}

// ============================================================================
// Tier 3: Simple Command Parsers
// ============================================================================

/// Parse any single token
pub fn any<'a>() -> impl Parser<&'a [Token], &'a Token, PError> {
    take(1usize).map(|slice: &[Token]| &slice[0])
}

/// Parse command name (non-reserved word)
/// cmd_name is just an alias for non_reserved_word in POSIX grammar
pub fn cmd_name<'a>() -> impl Parser<&'a [Token], &'a Token, PError> {
    non_reserved_word()
}

/// Parse command word - a non-reserved word that is NOT an assignment
/// This is used after cmd_prefix to get the actual command name
pub fn cmd_word<'a>() -> impl Parser<&'a [Token], &'a Token, PError> {
    // Optimized: Fast path check instead of expensive parse_scalar_assignment()
    word().verify(|tok: &&Token| {
        let s = tok.to_str();

        // Must not be a reserved word
        if is_reserved_word(s) {
            return false;
        }

        // Fast check: does it look like an assignment?
        // If no '=', definitely not an assignment
        if !s.contains('=') {
            return true;
        }

        // Has '=', need to check if it's a valid assignment pattern
        // Valid patterns: VAR=value, VAR+=value, VAR[idx]=value

        // Split at first '=' to check assignment pattern
        let (before_eq, _value) = match s.split_once('=') {
            Some(parts) => parts,
            None => return true, // No '=' found, not an assignment
        };

        // Check for VAR+= (append assignment)
        let name_part = if let Some(name) = before_eq.strip_suffix('+') {
            if name.is_empty() {
                return true; // Just '+=' is not valid
            }
            name
        } else if let Some(bracket) = before_eq.find('[') {
            // Check for VAR[idx]= (array element assignment)
            if let Some(with_bracket) = before_eq.strip_suffix(']') {
                &with_bracket[..bracket]
            } else {
                // Malformed, not valid assignment
                return true;
            }
        } else {
            before_eq
        };

        // If name_part is a valid identifier, it's an assignment - reject it
        // If not valid, it's just a word with '=' in it - accept it
        !is_valid_name(name_part)
    })
}

/// A single prefix item: either an I/O redirect or an assignment
fn cmd_prefix_item<'a>() -> impl Parser<&'a [Token], crate::ast::CommandPrefixOrSuffixItem, PError>
{
    // Try assignment first (more common in prefix: VAR=value cmd)
    // then redirect (less common in prefix: 2>&1 cmd)
    alt((
        assignment_word().map(|(a, w)| crate::ast::CommandPrefixOrSuffixItem::AssignmentWord(a, w)),
        io_redirect().map(crate::ast::CommandPrefixOrSuffixItem::IoRedirect),
    ))
}

/// Parse command prefix (one or more assignments and/or redirects before command name)
pub fn cmd_prefix<'a>() -> impl Parser<&'a [Token], crate::ast::CommandPrefix, PError> {
    repeat(1.., cmd_prefix_item()).map(crate::ast::CommandPrefix)
}

/// A single suffix item: I/O redirect, assignment, or word
fn cmd_suffix_item<'a>() -> impl Parser<&'a [Token], crate::ast::CommandPrefixOrSuffixItem, PError>
{
    // Optimization: Words are ~90% of suffix items, but redirects/assignments are ~10%
    // Use lookahead to avoid trying expensive parsers on common case
    |input: &mut &'a [Token]| {
        if looks_like_io_redirect(input) {
            // Rare case: try redirect first
            alt((
                io_redirect().map(crate::ast::CommandPrefixOrSuffixItem::IoRedirect),
                assignment_word()
                    .map(|(a, w)| crate::ast::CommandPrefixOrSuffixItem::AssignmentWord(a, w)),
                word().map(|tok| {
                    crate::ast::CommandPrefixOrSuffixItem::Word(crate::ast::Word::from(tok))
                }),
            ))
            .parse_next(input)
        } else {
            // Common case: try word first (most arguments are plain words)
            alt((
                word().map(|tok| {
                    crate::ast::CommandPrefixOrSuffixItem::Word(crate::ast::Word::from(tok))
                }),
                assignment_word()
                    .map(|(a, w)| crate::ast::CommandPrefixOrSuffixItem::AssignmentWord(a, w)),
                io_redirect().map(crate::ast::CommandPrefixOrSuffixItem::IoRedirect),
            ))
            .parse_next(input)
        }
    }
}

/// Parse command suffix (arguments, redirects, and assignments after command name)
pub fn cmd_suffix<'a>() -> impl Parser<&'a [Token], crate::ast::CommandSuffix, PError> {
    repeat(1.., cmd_suffix_item()).map(crate::ast::CommandSuffix)
}

/// Parse a simple command
/// Grammar: cmd_prefix (cmd_word cmd_suffix?)? | cmd_name cmd_suffix?
pub fn simple_command<'a>() -> impl Parser<&'a [Token], crate::ast::SimpleCommand, PError> {
    // Use lookahead to determine which form to try first
    // If the first token looks like an assignment or redirect, try Form 1 (prefix) first
    // Otherwise try Form 2 (name) first (most common case: echo hello, ls -la, etc.)
    |input: &mut &'a [Token]| {
        let try_prefix_first = if let Some(first) = input.first() {
            // Check if it's a redirect
            if looks_like_io_redirect(input) {
                true
            } else if let Token::Word(w, _) = first {
                // Check if it looks like an assignment (contains '=')
                w.contains('=')
            } else {
                false
            }
        } else {
            false
        };

        if try_prefix_first {
            // Try prefix form first (assignments/redirects before command)
            alt((
                (
                    cmd_prefix(),
                    winnow::combinator::opt((cmd_word(), winnow::combinator::opt(cmd_suffix()))),
                )
                    .map(|(prefix, word_and_suffix)| {
                        let (word_or_name, suffix) = match word_and_suffix {
                            Some((w, s)) => (Some(crate::ast::Word::from(w)), s),
                            None => (None, None),
                        };
                        crate::ast::SimpleCommand {
                            prefix: Some(prefix),
                            word_or_name,
                            suffix,
                        }
                    }),
                (cmd_name(), winnow::combinator::opt(cmd_suffix())).map(|(name, suffix)| {
                    crate::ast::SimpleCommand {
                        prefix: None,
                        word_or_name: Some(crate::ast::Word::from(name)),
                        suffix,
                    }
                }),
            ))
            .parse_next(input)
        } else {
            // Try name form first (most common: echo hello, ls -la, etc.)
            alt((
                (cmd_name(), winnow::combinator::opt(cmd_suffix())).map(|(name, suffix)| {
                    crate::ast::SimpleCommand {
                        prefix: None,
                        word_or_name: Some(crate::ast::Word::from(name)),
                        suffix,
                    }
                }),
                (
                    cmd_prefix(),
                    winnow::combinator::opt((cmd_word(), winnow::combinator::opt(cmd_suffix()))),
                )
                    .map(|(prefix, word_and_suffix)| {
                        let (word_or_name, suffix) = match word_and_suffix {
                            Some((w, s)) => (Some(crate::ast::Word::from(w)), s),
                            None => (None, None),
                        };
                        crate::ast::SimpleCommand {
                            prefix: Some(prefix),
                            word_or_name,
                            suffix,
                        }
                    }),
            ))
            .parse_next(input)
        }
    }
}

// ============================================================================
// Tier 3: Special Commands (arithmetic, extended test)
// ============================================================================

/// Parse arithmetic expression - collects tokens between (( and ))
/// Handles nested parentheses and stops at )) or ; (for arithmetic for loops)
pub fn arithmetic_expression<'a>()
-> impl Parser<&'a [Token], crate::ast::UnexpandedArithmeticExpr, PError> {
    move |input: &mut &'a [Token]| {
        let mut tokens_str = String::new();
        let mut paren_depth = 0;
        let mut last_was_word = false;

        loop {
            // Check for end: "))" or ";" at depth 0
            if paren_depth == 0 {
                // Check for "))"
                if input.len() >= 2 {
                    if let (Some(Token::Operator(s1, _)), Some(Token::Operator(s2, _))) =
                        (input.first(), input.get(1))
                    {
                        if s1 == ")" && s2 == ")" {
                            break;
                        }
                    }
                }
                // Check for ";" (arithmetic for loop separator)
                if let Some(Token::Operator(s, _)) = input.first() {
                    if s == ";" {
                        break;
                    }
                }
            }

            if input.is_empty() {
                break;
            }

            let tok = any().parse_next(input)?;
            match tok {
                Token::Operator(s, _) => {
                    if s == "(" {
                        paren_depth += 1;
                    } else if s == ")" {
                        paren_depth -= 1;
                    }
                    tokens_str.push_str(s);
                    last_was_word = false;
                }
                Token::Word(s, _) => {
                    if last_was_word && !tokens_str.is_empty() {
                        tokens_str.push(' ');
                    }
                    tokens_str.push_str(s);
                    last_was_word = true;
                }
            }
        }

        Ok(crate::ast::UnexpandedArithmeticExpr { value: tokens_str })
    }
}

/// Parse arithmetic command (( expr ))
pub fn arithmetic_command<'a>() -> impl Parser<&'a [Token], crate::ast::ArithmeticCommand, PError> {
    (
        matches_operator("("),
        matches_operator("("),
        arithmetic_expression(),
        matches_operator(")"),
        matches_operator(")"),
    )
        .map(|(start, _, expr, _, end)| {
            let loc = crate::SourceSpan::within(start.location(), end.location());
            crate::ast::ArithmeticCommand { expr, loc }
        })
}

/// Parse unary test operator (-e, -f, -d, etc.)
fn parse_unary_operator(op: &str) -> Option<crate::ast::UnaryPredicate> {
    use crate::ast::UnaryPredicate;
    match op {
        "-e" => Some(UnaryPredicate::FileExists),
        "-b" => Some(UnaryPredicate::FileExistsAndIsBlockSpecialFile),
        "-c" => Some(UnaryPredicate::FileExistsAndIsCharSpecialFile),
        "-d" => Some(UnaryPredicate::FileExistsAndIsDir),
        "-f" => Some(UnaryPredicate::FileExistsAndIsRegularFile),
        "-g" => Some(UnaryPredicate::FileExistsAndIsSetgid),
        "-h" | "-L" => Some(UnaryPredicate::FileExistsAndIsSymlink),
        "-k" => Some(UnaryPredicate::FileExistsAndHasStickyBit),
        "-p" => Some(UnaryPredicate::FileExistsAndIsFifo),
        "-r" => Some(UnaryPredicate::FileExistsAndIsReadable),
        "-s" => Some(UnaryPredicate::FileExistsAndIsNotZeroLength),
        "-t" => Some(UnaryPredicate::FdIsOpenTerminal),
        "-u" => Some(UnaryPredicate::FileExistsAndIsSetuid),
        "-w" => Some(UnaryPredicate::FileExistsAndIsWritable),
        "-x" => Some(UnaryPredicate::FileExistsAndIsExecutable),
        "-G" => Some(UnaryPredicate::FileExistsAndOwnedByEffectiveGroupId),
        "-N" => Some(UnaryPredicate::FileExistsAndModifiedSinceLastRead),
        "-O" => Some(UnaryPredicate::FileExistsAndOwnedByEffectiveUserId),
        "-S" => Some(UnaryPredicate::FileExistsAndIsSocket),
        "-o" => Some(UnaryPredicate::ShellOptionEnabled),
        "-v" => Some(UnaryPredicate::ShellVariableIsSetAndAssigned),
        "-R" => Some(UnaryPredicate::ShellVariableIsSetAndNameRef),
        "-z" => Some(UnaryPredicate::StringHasZeroLength),
        "-n" => Some(UnaryPredicate::StringHasNonZeroLength),
        _ => None,
    }
}

/// Parse binary test operator (=, !=, -eq, -lt, etc.)
fn parse_binary_operator(op: &str) -> Option<crate::ast::BinaryPredicate> {
    use crate::ast::BinaryPredicate;
    match op {
        "=" | "==" => Some(BinaryPredicate::StringExactlyMatchesString),
        "!=" => Some(BinaryPredicate::StringDoesNotExactlyMatchString),
        "<" => Some(BinaryPredicate::LeftSortsBeforeRight),
        ">" => Some(BinaryPredicate::LeftSortsAfterRight),
        "-eq" => Some(BinaryPredicate::ArithmeticEqualTo),
        "-ne" => Some(BinaryPredicate::ArithmeticNotEqualTo),
        "-lt" => Some(BinaryPredicate::ArithmeticLessThan),
        "-le" => Some(BinaryPredicate::ArithmeticLessThanOrEqualTo),
        "-gt" => Some(BinaryPredicate::ArithmeticGreaterThan),
        "-ge" => Some(BinaryPredicate::ArithmeticGreaterThanOrEqualTo),
        "-nt" => Some(BinaryPredicate::LeftFileIsNewerOrExistsWhenRightDoesNot),
        "-ot" => Some(BinaryPredicate::LeftFileIsOlderOrDoesNotExistWhenRightDoes),
        "-ef" => Some(BinaryPredicate::FilesReferToSameDeviceAndInodeNumbers),
        "=~" => Some(BinaryPredicate::StringMatchesRegex),
        _ => None,
    }
}

/// Parse extended test tokens into expression tree
fn parse_extended_test_tokens(
    tokens: &[Token],
) -> Result<crate::ast::ExtendedTestExpr, winnow::error::ErrMode<ContextError>> {
    use crate::ast::{ExtendedTestExpr, UnaryPredicate, Word};

    if tokens.is_empty() {
        return backtrack_error();
    }

    // Try to parse as unary test (operator + operand)
    if tokens.len() >= 2 {
        if let Some(Token::Word(op, _)) = tokens.first() {
            if let Some(unary_pred) = parse_unary_operator(op.as_str()) {
                if let Some(Token::Word(operand, loc)) = tokens.get(1) {
                    return Ok(ExtendedTestExpr::UnaryTest(
                        unary_pred,
                        Word::with_location(operand, loc),
                    ));
                }
            }
        }
    }

    // Try to parse as binary test (operand + operator + operand)
    if tokens.len() >= 3 {
        if let (
            Some(Token::Word(left, left_loc)),
            Some(Token::Word(op, _)),
            Some(Token::Word(right, right_loc)),
        ) = (tokens.first(), tokens.get(1), tokens.get(2))
        {
            if let Some(binary_pred) = parse_binary_operator(op.as_str()) {
                return Ok(ExtendedTestExpr::BinaryTest(
                    binary_pred,
                    Word::with_location(left, left_loc),
                    Word::with_location(right, right_loc),
                ));
            }
        }
    }

    // Fallback: treat single token as non-zero length string test
    if let Some(Token::Word(w, l)) = tokens.first() {
        Ok(ExtendedTestExpr::UnaryTest(
            UnaryPredicate::StringHasNonZeroLength,
            Word::with_location(w, l),
        ))
    } else {
        backtrack_error()
    }
}

/// Parse extended test command [[ ... ]]
pub fn extended_test_command<'a>()
-> impl Parser<&'a [Token], crate::ast::ExtendedTestExprCommand, PError> {
    move |input: &mut &'a [Token]| {
        let start = matches_word("[[").parse_next(input)?;

        // Collect all tokens until ]]
        let mut test_tokens = vec![];
        loop {
            if matches_word("]]").parse_next(input).is_ok() {
                break;
            }

            if input.is_empty() {
                return backtrack_error();
            }

            test_tokens.push(any().parse_next(input)?.clone());
        }

        let end = test_tokens.last().unwrap_or(&start);

        // Parse the test expression
        let expr = parse_extended_test_tokens(&test_tokens)?;

        let loc = crate::SourceSpan::within(start.location(), end.location());

        Ok(crate::ast::ExtendedTestExprCommand { expr, loc })
    }
}

// ============================================================================
// Tier 4: Compound Commands (require context via StatefulStream)
// ============================================================================

/// Type alias for stateful parser error
type StatefulError = winnow::error::ErrMode<ContextError>;

/// Parse do group (do ... done)
pub fn do_group<'a>() -> impl Parser<StatefulStream<'a>, crate::ast::DoGroupCommand, StatefulError>
{
    move |input: &mut StatefulStream<'a>| {
        let start = matches_word("do").parse_next(&mut input.input)?;
        let list = compound_list().parse_next(input)?;
        let end = matches_word("done").parse_next(&mut input.input)?;

        let loc = crate::SourceSpan::within(start.location(), end.location());

        Ok(crate::ast::DoGroupCommand { list, loc })
    }
}

/// Parse compound list (for use inside compound commands)
pub fn compound_list<'a>()
-> impl Parser<StatefulStream<'a>, crate::ast::CompoundList, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        linebreak().parse_next(&mut input.input)?;

        let first = and_or().parse_next(input)?;
        let mut and_ors = vec![first];
        let mut seps = vec![];

        // Parse (separator?, and_or) pairs using repeat to handle checkpointing automatically
        let pairs: Vec<(Option<SeparatorOperator>, crate::ast::AndOrList)> =
            winnow::combinator::repeat(0.., |input: &mut StatefulStream<'a>| {
                let sep = winnow::combinator::opt(separator()).parse_next(&mut input.input)?;
                let ao = and_or().parse_next(input)?;
                Ok((sep.flatten(), ao))
            })
            .parse_next(input)?;

        for (sep_opt, ao) in pairs {
            seps.push(sep_opt.unwrap_or(SeparatorOperator::Sequence));
            and_ors.push(ao);
        }

        // Handle optional trailing separator
        if let Ok(sep_opt) = winnow::combinator::opt(separator()).parse_next(&mut input.input) {
            seps.push(sep_opt.flatten().unwrap_or(SeparatorOperator::Sequence));
        }

        if seps.len() < and_ors.len() {
            seps.push(SeparatorOperator::Sequence);
        }

        let items = and_ors
            .into_iter()
            .enumerate()
            .map(|(i, ao)| crate::ast::CompoundListItem(ao, seps[i].clone()))
            .collect();

        Ok(crate::ast::CompoundList(items))
    }
}

/// Parse an and/or continuation: && or || followed by pipeline
fn and_or_continuation<'a>() -> impl Parser<StatefulStream<'a>, crate::ast::AndOr, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        linebreak().parse_next(&mut input.input)?;
        let op_fn = and_or_op().parse_next(&mut input.input)?;
        linebreak().parse_next(&mut input.input)?;
        let p = pipeline().parse_next(input)?;
        Ok(op_fn(p))
    }
}

/// Parse and_or list (pipelines connected with && or ||)
pub fn and_or<'a>() -> impl Parser<StatefulStream<'a>, crate::ast::AndOrList, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        let first = pipeline().parse_next(input)?;
        let additional: Vec<crate::ast::AndOr> =
            repeat(0.., and_or_continuation()).parse_next(input)?;

        Ok(crate::ast::AndOrList { first, additional })
    }
}

/// Parse pipeline with optional time prefix and negation
pub fn pipeline<'a>() -> impl Parser<StatefulStream<'a>, crate::ast::Pipeline, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        // Try timed pipeline prefix
        let timed = winnow::combinator::opt(pipeline_timed())
            .parse_next(&mut input.input)
            .ok()
            .flatten();

        // Parse ! tokens for negation (! is a word, not an operator, in shell tokenization)
        let mut bang = false;
        while matches_word("!").parse_next(&mut input.input).is_ok() {
            bang = true;
        }

        // Parse pipe sequence
        let seq = pipe_sequence().parse_next(input)?;

        Ok(crate::ast::Pipeline { timed, bang, seq })
    }
}

/// Parse compound command with optional redirects
fn compound_command_with_redirects<'a>()
-> impl Parser<StatefulStream<'a>, crate::ast::Command, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        let cmd = compound_command().parse_next(input)?;
        let redirects = winnow::combinator::opt(redirect_list()).parse_next(&mut input.input)?;
        Ok(crate::ast::Command::Compound(cmd, redirects))
    }
}

/// Parse a single command (simple command, compound command, or function definition)
/// Uses dispatch on first token for efficient parsing
pub fn command<'a>() -> impl Parser<StatefulStream<'a>, crate::ast::Command, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        let opts = input.state.options;

        // Peek at first token to guide dispatch
        let first_token_str = match input.input.first() {
            Some(tok) => tok.to_str(),
            None => return backtrack_error(),
        };

        match first_token_str {
            // "function" keyword -> must be function definition
            "function" => function_definition()
                .map(crate::ast::Command::Function)
                .parse_next(input),

            // Compound command keywords -> compound command with redirects
            "{" | "(" | "for" | "case" | "if" | "while" | "until" => {
                let cmd = compound_command().parse_next(input)?;
                let redirects =
                    winnow::combinator::opt(redirect_list()).parse_next(&mut input.input)?;
                Ok(crate::ast::Command::Compound(cmd, redirects))
            }

            // Extended test [[ -> extended test command (if enabled)
            "[[" if !opts.posix_mode && !opts.sh_mode => extended_test_command()
                .map(|cmd| crate::ast::Command::ExtendedTest(cmd, None))
                .parse_next(&mut input.input),

            // Everything else: check if it could be function definition (name() form)
            _ => {
                // Peek at second token - if it's "(", this could be a function definition
                let could_be_function = matches!(
                    input.input.get(1),
                    Some(Token::Operator(s, _)) if s == "("
                );

                if could_be_function {
                    // Try function definition first
                    if let Ok(func) = function_definition().parse_next(input) {
                        return Ok(crate::ast::Command::Function(func));
                    }
                    // function_definition failing here means malformed input
                    // fall through to simple_command which will also fail or partially parse
                }

                // Simple command (most common case)
                simple_command()
                    .map(crate::ast::Command::Simple)
                    .parse_next(&mut input.input)
            }
        }
    }
}

/// Parse brace group { ... }
pub fn brace_group<'a>()
-> impl Parser<StatefulStream<'a>, crate::ast::BraceGroupCommand, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        let start = matches_word("{").parse_next(&mut input.input)?;
        let list = compound_list().parse_next(input)?;
        let end = matches_word("}").parse_next(&mut input.input)?;

        let loc = crate::SourceSpan::within(start.location(), end.location());

        Ok(crate::ast::BraceGroupCommand { list, loc })
    }
}

/// Parse subshell ( ... )
pub fn subshell<'a>() -> impl Parser<StatefulStream<'a>, crate::ast::SubshellCommand, StatefulError>
{
    move |input: &mut StatefulStream<'a>| {
        let start = matches_operator("(").parse_next(&mut input.input)?;
        let list = compound_list().parse_next(input)?;
        let end = matches_operator(")").parse_next(&mut input.input)?;

        let loc = crate::SourceSpan::within(start.location(), end.location());

        Ok(crate::ast::SubshellCommand { list, loc })
    }
}

/// Parse for clause
pub fn for_clause<'a>()
-> impl Parser<StatefulStream<'a>, crate::ast::ForClauseCommand, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        let start = matches_word("for").parse_next(&mut input.input)?;
        let var_name = name().parse_next(&mut input.input)?;

        linebreak().parse_next(&mut input.input)?;

        // Optional "in" wordlist
        let values = if matches_word("in").parse_next(&mut input.input).is_ok() {
            winnow::combinator::opt(wordlist()).parse_next(&mut input.input)?
        } else {
            None
        };

        sequential_sep().parse_next(&mut input.input)?;
        let body = do_group().parse_next(input)?;

        let loc = crate::SourceSpan::within(start.location(), &body.loc);

        Ok(crate::ast::ForClauseCommand {
            variable_name: var_name.to_string(),
            values,
            body,
            loc,
        })
    }
}

/// Parse arithmetic for clause: for (( init; cond; update )) body
pub fn arithmetic_for_clause<'a>()
-> impl Parser<StatefulStream<'a>, crate::ast::ArithmeticForClauseCommand, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        let start = matches_word("for").parse_next(&mut input.input)?;

        matches_operator("(").parse_next(&mut input.input)?;
        matches_operator("(").parse_next(&mut input.input)?;

        // Parse three arithmetic expressions separated by ;
        let initializer =
            winnow::combinator::opt(arithmetic_expression()).parse_next(&mut input.input)?;
        matches_operator(";").parse_next(&mut input.input)?;

        let condition =
            winnow::combinator::opt(arithmetic_expression()).parse_next(&mut input.input)?;
        matches_operator(";").parse_next(&mut input.input)?;

        let updater =
            winnow::combinator::opt(arithmetic_expression()).parse_next(&mut input.input)?;

        matches_operator(")").parse_next(&mut input.input)?;
        matches_operator(")").parse_next(&mut input.input)?;

        linebreak().parse_next(&mut input.input)?;

        let body = arithmetic_for_body().parse_next(input)?;

        let loc = crate::SourceSpan::within(start.location(), &body.loc);

        Ok(crate::ast::ArithmeticForClauseCommand {
            initializer,
            condition,
            updater,
            body,
            loc,
        })
    }
}

/// Parse arithmetic for body (do_group or brace_group)
// TODO peek() on sequential_sep()
fn arithmetic_for_body<'a>()
-> impl Parser<StatefulStream<'a>, crate::ast::DoGroupCommand, StatefulError> {
    winnow::combinator::alt((
        // Try sequential_sep + do_group
        |input: &mut StatefulStream<'a>| {
            sequential_sep().parse_next(&mut input.input)?;
            do_group().parse_next(input)
        },
        // Try brace_group (convert to DoGroupCommand)
        brace_group().map(|bg| crate::ast::DoGroupCommand {
            list: bg.list,
            loc: bg.loc,
        }),
    ))
}

/// Parse case clause
pub fn case_clause<'a>()
-> impl Parser<StatefulStream<'a>, crate::ast::CaseClauseCommand, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        let start = matches_word("case").parse_next(&mut input.input)?;
        let target = word().parse_next(&mut input.input)?;

        linebreak().parse_next(&mut input.input)?;
        matches_word("in").parse_next(&mut input.input)?;
        linebreak().parse_next(&mut input.input)?;

        // Use opt() for optional case list
        let items = winnow::combinator::opt(case_list()).parse_next(input)?;

        let end = matches_word("esac").parse_next(&mut input.input)?;

        let loc = crate::SourceSpan::within(start.location(), end.location());

        Ok(crate::ast::CaseClauseCommand {
            value: crate::ast::Word::from(target),
            cases: items.unwrap_or_default(),
            loc,
        })
    }
}

/// Parse case list
fn case_list<'a>() -> impl Parser<StatefulStream<'a>, Vec<crate::ast::CaseItem>, StatefulError> {
    // Use repeat_till to collect case_items until we peek "esac"
    // The peek ensures we don't consume "esac" - case_clause will handle it
    winnow::combinator::repeat_till(
        1..,
        case_item(),
        winnow::combinator::peek(|input: &mut StatefulStream<'a>| {
            matches_word("esac").parse_next(&mut input.input)
        }),
    )
    .map(|(items, _): (Vec<_>, _)| items)
}

/// Parse case item terminator (;;, ;&, or ;;&)
fn case_item_terminator<'a>() -> impl Parser<&'a [Token], crate::ast::CaseItemPostAction, PError> {
    winnow::combinator::opt(dispatch! { peek_operator();
        ";;&" => op_value(";;&", crate::ast::CaseItemPostAction::ContinueEvaluatingCases),
        ";;" => op_value(";;", crate::ast::CaseItemPostAction::ExitCase),
        ";&" => op_value(";&", crate::ast::CaseItemPostAction::UnconditionallyExecuteNextCaseItem),
        _ => fail,
    })
    .map(|opt| opt.unwrap_or(crate::ast::CaseItemPostAction::ExitCase))
}

/// Parse case item
fn case_item<'a>() -> impl Parser<StatefulStream<'a>, crate::ast::CaseItem, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        // Optional leading (
        let _ = matches_operator("(").parse_next(&mut input.input).ok();

        // Parse patterns: word separated by |
        let patterns: Vec<crate::ast::Word> =
            winnow::combinator::separated(1.., word_as_ast(), matches_operator("|"))
                .parse_next(&mut input.input)?;

        matches_operator(")").parse_next(&mut input.input)?;

        linebreak().parse_next(&mut input.input)?;

        // Parse body (optional)
        let cmd = winnow::combinator::opt(compound_list()).parse_next(input)?;

        // Parse case item terminator
        let post_action = case_item_terminator().parse_next(&mut input.input)?;

        linebreak().parse_next(&mut input.input)?;

        Ok(crate::ast::CaseItem {
            patterns,
            cmd,
            post_action,
            loc: None,
        })
    }
}

/// Parse an elif clause
fn elif_clause<'a>() -> impl Parser<StatefulStream<'a>, crate::ast::ElseClause, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        matches_word("elif").parse_next(&mut input.input)?;
        let condition = compound_list().parse_next(input)?;
        matches_word("then").parse_next(&mut input.input)?;
        let body = compound_list().parse_next(input)?;
        Ok(crate::ast::ElseClause {
            condition: Some(condition),
            body,
        })
    }
}

/// Parse an else clause (final, no condition)
fn else_clause<'a>() -> impl Parser<StatefulStream<'a>, crate::ast::ElseClause, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        matches_word("else").parse_next(&mut input.input)?;
        let body = compound_list().parse_next(input)?;
        Ok(crate::ast::ElseClause {
            condition: None,
            body,
        })
    }
}

/// Parse if clause
pub fn if_clause<'a>() -> impl Parser<StatefulStream<'a>, crate::ast::IfClauseCommand, StatefulError>
{
    move |input: &mut StatefulStream<'a>| {
        let start = matches_word("if").parse_next(&mut input.input)?;

        let condition = compound_list().parse_next(input)?;

        matches_word("then").parse_next(&mut input.input)?;

        let then_body = compound_list().parse_next(input)?;

        // Parse elif clauses (zero or more)
        let mut elses: Vec<crate::ast::ElseClause> =
            repeat(0.., elif_clause()).parse_next(input)?;

        // Parse optional else clause
        if let Ok(else_part) = else_clause().parse_next(input) {
            elses.push(else_part);
        }

        let end = matches_word("fi").parse_next(&mut input.input)?;

        let loc = crate::SourceSpan::within(start.location(), end.location());

        Ok(crate::ast::IfClauseCommand {
            condition,
            then: then_body,
            elses: if elses.is_empty() { None } else { Some(elses) },
            loc,
        })
    }
}

/// Parse while clause
pub fn while_clause<'a>()
-> impl Parser<StatefulStream<'a>, crate::ast::WhileOrUntilClauseCommand, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        let start = matches_word("while").parse_next(&mut input.input)?;
        let condition = compound_list().parse_next(input)?;
        let body = do_group().parse_next(input)?;

        let loc = crate::SourceSpan::within(start.location(), &body.loc);

        Ok(crate::ast::WhileOrUntilClauseCommand(condition, body, loc))
    }
}

/// Parse until clause
pub fn until_clause<'a>()
-> impl Parser<StatefulStream<'a>, crate::ast::WhileOrUntilClauseCommand, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        let start = matches_word("until").parse_next(&mut input.input)?;
        let condition = compound_list().parse_next(input)?;
        let body = do_group().parse_next(input)?;

        let loc = crate::SourceSpan::within(start.location(), &body.loc);

        Ok(crate::ast::WhileOrUntilClauseCommand(condition, body, loc))
    }
}

/// Parse function definition
pub fn function_definition<'a>()
-> impl Parser<StatefulStream<'a>, crate::ast::FunctionDefinition, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        // Try "function name () body" or "function name body" format
        let has_function_keyword = matches_word("function")
            .parse_next(&mut input.input)
            .is_ok();

        let fname = name().parse_next(&mut input.input)?;

        // Parse optional ()
        let has_parens = if matches_operator("(").parse_next(&mut input.input).is_ok() {
            matches_operator(")").parse_next(&mut input.input)?;
            true
        } else {
            false
        };

        // Must have either "function" keyword or parens
        if !has_function_keyword && !has_parens {
            return backtrack_error();
        }

        linebreak().parse_next(&mut input.input)?;

        let body = function_body().parse_next(input)?;

        Ok(crate::ast::FunctionDefinition {
            fname: crate::ast::Word::from(fname.to_string()),
            body,
        })
    }
}

/// Parse function body (compound command with optional redirects)
fn function_body<'a>() -> impl Parser<StatefulStream<'a>, crate::ast::FunctionBody, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        let cmd = compound_command().parse_next(input)?;
        let redirects = winnow::combinator::opt(redirect_list()).parse_next(&mut input.input)?;

        Ok(crate::ast::FunctionBody(cmd, redirects))
    }
}

/// Helper to parse for clause or arithmetic for clause based on what follows "for"
fn for_or_arithmetic_for<'a>()
-> impl Parser<StatefulStream<'a>, crate::ast::CompoundCommand, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        let opts = input.state.options;

        if opts.posix_mode || opts.sh_mode {
            // POSIX mode: only regular for clause allowed
            for_clause()
                .map(crate::ast::CompoundCommand::ForClause)
                .parse_next(input)
        } else {
            // Bash mode: Optimization - use lookahead to avoid backtracking
            // Check if it's "for ((" pattern (arithmetic) or "for NAME" pattern (regular)
            // Skip the "for" keyword to peek at what follows
            let is_arithmetic = if let Some(Token::Word(w, _)) = input.input.first() {
                if w == "for" {
                    // Peek at token after "for"
                    matches!(input.input.get(1), Some(Token::Operator(op, _)) if op == "(")
                } else {
                    false
                }
            } else {
                false
            };

            if is_arithmetic {
                arithmetic_for_clause()
                    .map(crate::ast::CompoundCommand::ArithmeticForClause)
                    .parse_next(input)
            } else {
                for_clause()
                    .map(crate::ast::CompoundCommand::ForClause)
                    .parse_next(input)
            }
        }
    }
}

/// Helper to handle `(` in POSIX mode - only subshell allowed
fn paren_compound_posix<'a>()
-> impl Parser<StatefulStream<'a>, crate::ast::CompoundCommand, StatefulError> {
    subshell().map(crate::ast::CompoundCommand::Subshell)
}

/// Helper to handle `(` in Bash mode - could be subshell or arithmetic command `((`
fn paren_compound_bash<'a>()
-> impl Parser<StatefulStream<'a>, crate::ast::CompoundCommand, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        // Check for `((` pattern - could be arithmetic command
        let try_arith_first = if input.input.len() >= 2
            && matches!(input.input.get(1), Some(Token::Operator(s, _)) if s == "(")
        {
            // Peek at the third token to determine if it looks like a command
            let looks_like_command = match input.input.get(2) {
                Some(Token::Word(w, _)) => {
                    matches!(
                        w.as_str(),
                        "echo"
                            | "if"
                            | "while"
                            | "for"
                            | "case"
                            | "function"
                            | "return"
                            | "exit"
                            | "cd"
                            | "pwd"
                            | "ls"
                            | "cat"
                            | "grep"
                    )
                }
                _ => false,
            };
            !looks_like_command
        } else {
            false
        };

        if try_arith_first {
            // Try arithmetic first, then subshell
            winnow::combinator::alt((
                |i: &mut StatefulStream<'a>| {
                    arithmetic_command()
                        .parse_next(&mut i.input)
                        .map(crate::ast::CompoundCommand::Arithmetic)
                },
                subshell().map(crate::ast::CompoundCommand::Subshell),
            ))
            .parse_next(input)
        } else {
            // Try subshell first, then arithmetic
            winnow::combinator::alt((
                subshell().map(crate::ast::CompoundCommand::Subshell),
                |i: &mut StatefulStream<'a>| {
                    arithmetic_command()
                        .parse_next(&mut i.input)
                        .map(crate::ast::CompoundCommand::Arithmetic)
                },
            ))
            .parse_next(input)
        }
    }
}

/// Helper to handle `(` which could be subshell or arithmetic command `((`
fn paren_compound<'a>()
-> impl Parser<StatefulStream<'a>, crate::ast::CompoundCommand, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        let opts = input.state.options;

        if opts.posix_mode || opts.sh_mode {
            paren_compound_posix().parse_next(input)
        } else {
            paren_compound_bash().parse_next(input)
        }
    }
}

/// Parse compound command
/// Uses dispatch on first token for O(1) keyword lookup
pub fn compound_command<'a>()
-> impl Parser<StatefulStream<'a>, crate::ast::CompoundCommand, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        // Peek at first token to dispatch
        let first_token_str = match input.input.first() {
            Some(tok) => tok.to_str(),
            None => return backtrack_error(),
        };

        match first_token_str {
            // Brace group
            "{" => brace_group()
                .map(crate::ast::CompoundCommand::BraceGroup)
                .parse_next(input),

            // Parentheses - could be subshell or arithmetic
            "(" => paren_compound().parse_next(input),

            // Keyword-based compound commands
            "for" => for_or_arithmetic_for().parse_next(input),
            "case" => case_clause()
                .map(crate::ast::CompoundCommand::CaseClause)
                .parse_next(input),
            "if" => if_clause()
                .map(crate::ast::CompoundCommand::IfClause)
                .parse_next(input),
            "while" => while_clause()
                .map(crate::ast::CompoundCommand::WhileClause)
                .parse_next(input),
            "until" => until_clause()
                .map(crate::ast::CompoundCommand::UntilClause)
                .parse_next(input),

            // No match
            _ => Err(winnow::error::ErrMode::Backtrack(ContextError::default())),
        }
    }
}

// ============================================================================
// Tier 5: Top-Level Parsers (complete commands and programs)
// ============================================================================

/// Add stderr->stdout redirect to a command (for |& operator)
fn add_stderr_redirect(cmd: &mut crate::ast::Command) {
    let redirect = crate::ast::IoRedirect::File(
        Some(2), // FD 2 (stderr)
        crate::ast::IoFileRedirectKind::DuplicateOutput,
        crate::ast::IoFileRedirectTarget::Fd(1), // FD 1 (stdout)
    );

    match cmd {
        crate::ast::Command::Simple(simple) => {
            if let Some(suffix) = &mut simple.suffix {
                suffix
                    .0
                    .push(crate::ast::CommandPrefixOrSuffixItem::IoRedirect(redirect));
            } else {
                simple.suffix = Some(crate::ast::CommandSuffix(vec![
                    crate::ast::CommandPrefixOrSuffixItem::IoRedirect(redirect),
                ]));
            }
        }
        crate::ast::Command::Compound(_, redirects) => {
            if let Some(rlist) = redirects {
                rlist.0.push(redirect);
            } else {
                *redirects = Some(crate::ast::RedirectList(vec![redirect]));
            }
        }
        crate::ast::Command::Function(func) => {
            // Add redirect to function body
            if let Some(rlist) = &mut func.body.1 {
                rlist.0.push(redirect);
            } else {
                func.body.1 = Some(crate::ast::RedirectList(vec![redirect]));
            }
        }
        crate::ast::Command::ExtendedTest(_, rlist) => {
            // Add redirect to extended test
            if let Some(rlist) = rlist {
                rlist.0.push(redirect);
            } else {
                *rlist = Some(crate::ast::RedirectList(vec![redirect]));
            }
        }
    }
}

/// Parse pipeline timing prefix ("time" or "time -p")
pub fn pipeline_timed<'a>() -> impl Parser<&'a [Token], crate::ast::PipelineTimed, PError> {
    move |input: &mut &'a [Token]| {
        let start = matches_word("time").parse_next(input)?;
        let loc = start.location().clone();

        if matches_word("-p").parse_next(input).is_ok() {
            Ok(crate::ast::PipelineTimed::TimedWithPosixOutput(loc))
        } else {
            Ok(crate::ast::PipelineTimed::Timed(loc))
        }
    }
}

/// Parse pipe sequence (commands separated by | or |&)
pub fn pipe_sequence<'a>()
-> impl Parser<StatefulStream<'a>, Vec<crate::ast::Command>, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        let mut commands = vec![command().parse_next(input)?];

        // Parse remaining pipe segments, applying |& redirects inline
        loop {
            // Save checkpoint before trying pipe operator
            let checkpoint = input.checkpoint();

            let Ok(is_pipe_and) = pipe_operator().parse_next(&mut input.input) else {
                break;
            };

            // linebreak always succeeds (0 or more newlines)
            linebreak().parse_next(&mut input.input)?;

            // Try to parse the next command
            match command().parse_next(input) {
                Ok(cmd) => {
                    // If |& was used, add stderr->stdout redirect to previous command
                    if is_pipe_and {
                        if let Some(last_cmd) = commands.last_mut() {
                            add_stderr_redirect(last_cmd);
                        }
                    }
                    commands.push(cmd);
                }
                Err(_) => {
                    // No command after pipe - restore and break
                    input.reset(&checkpoint);
                    break;
                }
            }
        }

        Ok(commands)
    }
}

/// Parse a separator followed by and_or
fn separated_and_or<'a>()
-> impl Parser<StatefulStream<'a>, (SeparatorOperator, crate::ast::AndOrList), StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        let sep = separator_op().parse_next(&mut input.input)?;
        let ao = and_or().parse_next(input)?;
        Ok((sep, ao))
    }
}

/// Parse a complete command (and/or list with separator)
pub fn complete_command<'a>()
-> impl Parser<StatefulStream<'a>, crate::ast::CompleteCommand, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        // Parse (and_or, separator) pairs directly into items
        let mut items: Vec<crate::ast::CompoundListItem> =
            winnow::combinator::repeat(0.., |input: &mut StatefulStream<'a>| {
                let ao = and_or().parse_next(input)?;
                let sep = separator_op().parse_next(&mut input.input)?;
                Ok(crate::ast::CompoundListItem(ao, sep))
            })
            .parse_next(input)?;

        // Try to parse a final and_or (without trailing separator)
        if let Ok(final_ao) = and_or().parse_next(input) {
            items.push(crate::ast::CompoundListItem(
                final_ao,
                SeparatorOperator::Sequence,
            ));
        } else if items.is_empty() {
            // No items at all - must have at least one and_or
            return backtrack_error();
        }

        Ok(crate::ast::CompoundList(items))
    }
}

/// Parse a newline-separated complete command continuation
fn complete_command_continuation<'a>()
-> impl Parser<StatefulStream<'a>, crate::ast::CompleteCommand, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        newline_list().parse_next(&mut input.input)?;
        complete_command().parse_next(input)
    }
}

/// Parse multiple complete commands separated by newlines
pub fn complete_commands<'a>()
-> impl Parser<StatefulStream<'a>, Vec<crate::ast::CompleteCommand>, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        let first = complete_command().parse_next(input)?;
        let rest: Vec<crate::ast::CompleteCommand> =
            repeat(0.., complete_command_continuation()).parse_next(input)?;

        let mut commands = Vec::with_capacity(1 + rest.len());
        commands.push(first);
        commands.extend(rest);
        Ok(commands)
    }
}

/// Parse a complete program
pub fn program<'a>() -> impl Parser<StatefulStream<'a>, crate::ast::Program, StatefulError> {
    move |input: &mut StatefulStream<'a>| {
        linebreak().parse_next(&mut input.input)?;

        let complete_commands_result = {
            let checkpoint = input.checkpoint();
            match complete_commands().parse_next(input) {
                Ok(cmds) => Some(cmds),
                Err(_) => {
                    input.reset(&checkpoint);
                    None
                }
            }
        };

        linebreak().parse_next(&mut input.input)?;

        Ok(crate::ast::Program {
            complete_commands: complete_commands_result.unwrap_or_default(),
        })
    }
}

/// Public API entry point for parsing a program
///
/// This matches the baseline `parse_program` signature for API compatibility.
///
/// # Arguments
/// * `tokens` - Token stream to parse
/// * `options` - Parser options controlling behavior
/// * `source_info` - Source information for error reporting
///
/// # Returns
/// * `Ok(Program)` on success
/// * `Err(ParseError)` on parse failure with user-friendly error message
pub fn parse_program<'a>(
    tokens: &'a [Token],
    options: &'a ParserOptions,
    source_info: &'a SourceInfo,
) -> Result<crate::ast::Program, crate::ParseError> {
    let ctx = ParserContext {
        options,
        source_info,
    };
    let mut stream = make_stream(tokens, ctx);

    match program().parse_next(&mut stream) {
        Ok(prog) => Ok(prog),
        Err(_e) => {
            // Convert winnow error to ParseError
            // Use the remaining input to find the error location
            match stream.input.first() {
                Some(token) => Err(crate::ParseError::ParsingNearToken(token.clone())),
                None => Err(crate::ParseError::ParsingAtEndOfInput),
            }
        }
    }
}

// Test helper functions with common signatures
#[cfg(all(test, feature = "use-winnow-parser"))]
pub(super) fn test_case_clause(
    tokens: &[Token],
    options: &ParserOptions,
    source_info: &SourceInfo,
) -> Result<crate::ast::CaseClauseCommand, crate::ParseError> {
    let ctx = ParserContext {
        options,
        source_info,
    };
    let mut stream = make_stream(tokens, ctx);

    match case_clause().parse_next(&mut stream) {
        Ok(cmd) => Ok(cmd),
        Err(_e) => match stream.input.first() {
            Some(token) => Err(crate::ParseError::ParsingNearToken(token.clone())),
            None => Err(crate::ParseError::ParsingAtEndOfInput),
        },
    }
}

#[cfg(all(test, feature = "use-winnow-parser"))]
pub(super) fn test_program(
    tokens: &[Token],
    options: &ParserOptions,
    source_info: &SourceInfo,
) -> Result<crate::ast::Program, crate::ParseError> {
    let ctx = ParserContext {
        options,
        source_info,
    };
    let mut stream = make_stream(tokens, ctx);

    match program().parse_next(&mut stream) {
        Ok(prog) => Ok(prog),
        Err(_e) => match stream.input.first() {
            Some(token) => Err(crate::ParseError::ParsingNearToken(token.clone())),
            None => Err(crate::ParseError::ParsingAtEndOfInput),
        },
    }
}

#[cfg(all(test, feature = "use-winnow-parser"))]
pub(super) fn test_pipe_sequence(
    tokens: &[Token],
    options: &ParserOptions,
    source_info: &SourceInfo,
) -> Result<Vec<crate::ast::Command>, crate::ParseError> {
    let ctx = ParserContext {
        options,
        source_info,
    };
    let mut stream = make_stream(tokens, ctx);

    match pipe_sequence().parse_next(&mut stream) {
        Ok(seq) => Ok(seq),
        Err(_e) => match stream.input.first() {
            Some(token) => Err(crate::ParseError::ParsingNearToken(token.clone())),
            None => Err(crate::ParseError::ParsingAtEndOfInput),
        },
    }
}

#[cfg(all(test, feature = "use-winnow-parser"))]
mod tests {
    use super::*;
    use crate::{SourcePosition, SourceSpan};
    use std::sync::Arc;

    fn make_span() -> SourceSpan {
        let pos = Arc::new(SourcePosition {
            index: 0,
            line: 1,
            column: 1,
        });
        SourceSpan {
            start: pos.clone(),
            end: pos,
        }
    }

    fn op(s: &str) -> Token {
        Token::Operator(s.to_string(), make_span())
    }

    fn w(s: &str) -> Token {
        Token::Word(s.to_string(), make_span())
    }

    // ========================================================================
    // Tier 0 Tests: Pure Token Parsers
    // ========================================================================

    #[test]
    fn test_word_success() {
        let tokens = vec![w("hello")];
        let mut input = tokens.as_slice();
        let result = word().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_str(), "hello");
        assert!(input.is_empty());
    }

    #[test]
    fn test_word_fails_on_operator() {
        let tokens = vec![op(";")];
        let mut input = tokens.as_slice();
        let result = word().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_matches_operator_success() {
        let tokens = vec![op(";")];
        let mut input = tokens.as_slice();
        let result = matches_operator(";").parse_next(&mut input);
        assert!(result.is_ok());
        assert!(input.is_empty());
    }

    #[test]
    fn test_matches_operator_wrong_operator() {
        let tokens = vec![op("&")];
        let mut input = tokens.as_slice();
        let result = matches_operator(";").parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_matches_operator_fails_on_word() {
        let tokens = vec![w("hello")];
        let mut input = tokens.as_slice();
        let result = matches_operator(";").parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_matches_word_success() {
        let tokens = vec![w("if")];
        let mut input = tokens.as_slice();
        let result = matches_word("if").parse_next(&mut input);
        assert!(result.is_ok());
        assert!(input.is_empty());
    }

    #[test]
    fn test_matches_word_wrong_word() {
        let tokens = vec![w("else")];
        let mut input = tokens.as_slice();
        let result = matches_word("if").parse_next(&mut input);
        assert!(result.is_err());
    }

    // ========================================================================
    // Tier 1 Tests: Basic Combinators
    // ========================================================================

    #[test]
    fn test_linebreak_zero_newlines() {
        let tokens = vec![w("echo")];
        let mut input = tokens.as_slice();
        let result = linebreak().parse_next(&mut input);
        assert!(result.is_ok());
        // Should not consume the word
        assert_eq!(input.len(), 1);
    }

    #[test]
    fn test_linebreak_one_newline() {
        let tokens = vec![op("\n"), w("echo")];
        let mut input = tokens.as_slice();
        let result = linebreak().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(input.len(), 1);
        assert!(matches!(&input[0], Token::Word(s, _) if s == "echo"));
    }

    #[test]
    fn test_linebreak_multiple_newlines() {
        let tokens = vec![op("\n"), op("\n"), op("\n"), w("echo")];
        let mut input = tokens.as_slice();
        let result = linebreak().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(input.len(), 1);
    }

    #[test]
    fn test_newline_list_one_newline() {
        let tokens = vec![op("\n"), w("echo")];
        let mut input = tokens.as_slice();
        let result = newline_list().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(input.len(), 1);
    }

    #[test]
    fn test_newline_list_multiple_newlines() {
        let tokens = vec![op("\n"), op("\n"), w("echo")];
        let mut input = tokens.as_slice();
        let result = newline_list().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(input.len(), 1);
    }

    #[test]
    fn test_newline_list_requires_at_least_one() {
        let tokens = vec![w("echo")];
        let mut input = tokens.as_slice();
        let result = newline_list().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_separator_op_semicolon() {
        let tokens = vec![op(";")];
        let mut input = tokens.as_slice();
        let result = separator_op().parse_next(&mut input);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), SeparatorOperator::Sequence));
        assert!(input.is_empty());
    }

    #[test]
    fn test_separator_op_ampersand() {
        let tokens = vec![op("&")];
        let mut input = tokens.as_slice();
        let result = separator_op().parse_next(&mut input);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), SeparatorOperator::Async));
        assert!(input.is_empty());
    }

    #[test]
    fn test_separator_op_fails_on_other() {
        let tokens = vec![op("|")];
        let mut input = tokens.as_slice();
        let result = separator_op().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_separator_with_semicolon_and_linebreak() {
        let tokens = vec![op(";"), op("\n"), op("\n"), w("echo")];
        let mut input = tokens.as_slice();
        let result = separator().parse_next(&mut input);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Some(SeparatorOperator::Sequence)));
        assert_eq!(input.len(), 1); // only "echo" remains
    }

    #[test]
    fn test_separator_with_ampersand() {
        let tokens = vec![op("&"), w("echo")];
        let mut input = tokens.as_slice();
        let result = separator().parse_next(&mut input);
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Some(SeparatorOperator::Async)));
        assert_eq!(input.len(), 1);
    }

    #[test]
    fn test_separator_with_newline_only() {
        let tokens = vec![op("\n"), op("\n"), w("echo")];
        let mut input = tokens.as_slice();
        let result = separator().parse_next(&mut input);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none()); // None means just newlines
        assert_eq!(input.len(), 1);
    }

    #[test]
    fn test_sequential_sep_semicolon() {
        let tokens = vec![op(";"), op("\n"), w("echo")];
        let mut input = tokens.as_slice();
        let result = sequential_sep().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(input.len(), 1);
    }

    #[test]
    fn test_sequential_sep_newlines() {
        let tokens = vec![op("\n"), op("\n"), w("echo")];
        let mut input = tokens.as_slice();
        let result = sequential_sep().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(input.len(), 1);
    }

    #[test]
    fn test_sequential_sep_fails_on_ampersand() {
        let tokens = vec![op("&"), w("echo")];
        let mut input = tokens.as_slice();
        let result = sequential_sep().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_and_or_op_and() {
        let tokens = vec![op("&&")];
        let mut input = tokens.as_slice();
        let result = and_or_op().parse_next(&mut input);
        assert!(result.is_ok());
        assert!(input.is_empty());
        // The result is a function, we can't easily test it without a Pipeline
    }

    #[test]
    fn test_and_or_op_or() {
        let tokens = vec![op("||")];
        let mut input = tokens.as_slice();
        let result = and_or_op().parse_next(&mut input);
        assert!(result.is_ok());
        assert!(input.is_empty());
    }

    #[test]
    fn test_and_or_op_fails_on_single_pipe() {
        let tokens = vec![op("|")];
        let mut input = tokens.as_slice();
        let result = and_or_op().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_and_or_op_fails_on_single_ampersand() {
        let tokens = vec![op("&")];
        let mut input = tokens.as_slice();
        let result = and_or_op().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_pipe_operator_simple_pipe() {
        let tokens = vec![op("|")];
        let mut input = tokens.as_slice();
        let result = pipe_operator().parse_next(&mut input);
        assert!(result.is_ok());
        assert!(!result.unwrap()); // false = regular pipe
        assert!(input.is_empty());
    }

    #[test]
    fn test_pipe_operator_pipe_and() {
        let tokens = vec![op("|&")];
        let mut input = tokens.as_slice();
        let result = pipe_operator().parse_next(&mut input);
        assert!(result.is_ok());
        assert!(result.unwrap()); // true = |& pipe
        assert!(input.is_empty());
    }

    #[test]
    fn test_pipe_operator_fails_on_other() {
        let tokens = vec![op("&&")];
        let mut input = tokens.as_slice();
        let result = pipe_operator().parse_next(&mut input);
        assert!(result.is_err());
    }

    // ========================================================================
    // Tier 2 Tests: Word-related Parsers
    // ========================================================================

    #[test]
    fn test_wordlist_single_word() {
        let tokens = vec![w("hello")];
        let mut input = tokens.as_slice();
        let result = wordlist().parse_next(&mut input);
        assert!(result.is_ok());
        let words = result.unwrap();
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].value, "hello");
        assert!(input.is_empty());
    }

    #[test]
    fn test_wordlist_multiple_words() {
        let tokens = vec![w("a"), w("b"), w("c"), op(";")];
        let mut input = tokens.as_slice();
        let result = wordlist().parse_next(&mut input);
        assert!(result.is_ok());
        let words = result.unwrap();
        assert_eq!(words.len(), 3);
        assert_eq!(words[0].value, "a");
        assert_eq!(words[1].value, "b");
        assert_eq!(words[2].value, "c");
        assert_eq!(input.len(), 1); // semicolon remains
    }

    #[test]
    fn test_wordlist_requires_at_least_one() {
        let tokens = vec![op(";")];
        let mut input = tokens.as_slice();
        let result = wordlist().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_name_valid() {
        let tokens = vec![w("my_var")];
        let mut input = tokens.as_slice();
        let result = name().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "my_var");
    }

    #[test]
    fn test_name_with_underscore_start() {
        let tokens = vec![w("_private")];
        let mut input = tokens.as_slice();
        let result = name().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "_private");
    }

    #[test]
    fn test_name_with_numbers() {
        let tokens = vec![w("var123")];
        let mut input = tokens.as_slice();
        let result = name().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "var123");
    }

    #[test]
    fn test_name_invalid_starts_with_number() {
        let tokens = vec![w("123var")];
        let mut input = tokens.as_slice();
        let result = name().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_name_invalid_contains_dash() {
        let tokens = vec![w("my-var")];
        let mut input = tokens.as_slice();
        let result = name().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_non_reserved_word_success() {
        let tokens = vec![w("echo")];
        let mut input = tokens.as_slice();
        let result = non_reserved_word().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_str(), "echo");
    }

    #[test]
    fn test_non_reserved_word_fails_on_if() {
        let tokens = vec![w("if")];
        let mut input = tokens.as_slice();
        let result = non_reserved_word().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_non_reserved_word_fails_on_while() {
        let tokens = vec![w("while")];
        let mut input = tokens.as_slice();
        let result = non_reserved_word().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_non_reserved_word_fails_on_brace() {
        let tokens = vec![w("{")];
        let mut input = tokens.as_slice();
        let result = non_reserved_word().parse_next(&mut input);
        assert!(result.is_err());
    }

    // ========================================================================
    // Tier 2 Tests: I/O Redirect Parsers
    // ========================================================================

    #[test]
    fn test_io_number_success() {
        let tokens = vec![w("2"), op(">")];
        let mut input = tokens.as_slice();
        let result = io_number().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2);
        assert_eq!(input.len(), 1); // operator remains
    }

    #[test]
    fn test_io_number_fails_on_non_digit() {
        let tokens = vec![w("abc")];
        let mut input = tokens.as_slice();
        let result = io_number().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_io_filename() {
        let tokens = vec![w("output.txt")];
        let mut input = tokens.as_slice();
        let result = io_filename().parse_next(&mut input);
        assert!(result.is_ok());
        match result.unwrap() {
            crate::ast::IoFileRedirectTarget::Filename(w) => {
                assert_eq!(w.value, "output.txt");
            }
            _ => panic!("Expected Filename"),
        }
    }

    #[test]
    fn test_io_file_write() {
        let tokens = vec![op(">"), w("file.txt")];
        let mut input = tokens.as_slice();
        let result = io_file().parse_next(&mut input);
        assert!(result.is_ok());
        let (kind, target) = result.unwrap();
        assert!(matches!(kind, crate::ast::IoFileRedirectKind::Write));
        match target {
            crate::ast::IoFileRedirectTarget::Filename(w) => assert_eq!(w.value, "file.txt"),
            _ => panic!("Expected Filename"),
        }
    }

    #[test]
    fn test_io_file_read() {
        let tokens = vec![op("<"), w("input.txt")];
        let mut input = tokens.as_slice();
        let result = io_file().parse_next(&mut input);
        assert!(result.is_ok());
        let (kind, _) = result.unwrap();
        assert!(matches!(kind, crate::ast::IoFileRedirectKind::Read));
    }

    #[test]
    fn test_io_file_append() {
        let tokens = vec![op(">>"), w("log.txt")];
        let mut input = tokens.as_slice();
        let result = io_file().parse_next(&mut input);
        assert!(result.is_ok());
        let (kind, _) = result.unwrap();
        assert!(matches!(kind, crate::ast::IoFileRedirectKind::Append));
    }

    #[test]
    fn test_io_file_duplicate_output() {
        let tokens = vec![op(">&"), w("1")];
        let mut input = tokens.as_slice();
        let result = io_file().parse_next(&mut input);
        assert!(result.is_ok());
        let (kind, target) = result.unwrap();
        assert!(matches!(
            kind,
            crate::ast::IoFileRedirectKind::DuplicateOutput
        ));
        match target {
            crate::ast::IoFileRedirectTarget::Duplicate(w) => assert_eq!(w.value, "1"),
            _ => panic!("Expected Duplicate"),
        }
    }

    #[test]
    fn test_io_file_duplicate_input() {
        let tokens = vec![op("<&"), w("0")];
        let mut input = tokens.as_slice();
        let result = io_file().parse_next(&mut input);
        assert!(result.is_ok());
        let (kind, _) = result.unwrap();
        assert!(matches!(
            kind,
            crate::ast::IoFileRedirectKind::DuplicateInput
        ));
    }

    #[test]
    fn test_io_file_read_write() {
        let tokens = vec![op("<>"), w("file.txt")];
        let mut input = tokens.as_slice();
        let result = io_file().parse_next(&mut input);
        assert!(result.is_ok());
        let (kind, _) = result.unwrap();
        assert!(matches!(kind, crate::ast::IoFileRedirectKind::ReadAndWrite));
    }

    #[test]
    fn test_io_file_clobber() {
        let tokens = vec![op(">|"), w("file.txt")];
        let mut input = tokens.as_slice();
        let result = io_file().parse_next(&mut input);
        assert!(result.is_ok());
        let (kind, _) = result.unwrap();
        assert!(matches!(kind, crate::ast::IoFileRedirectKind::Clobber));
    }

    #[test]
    fn test_io_redirect_simple() {
        let tokens = vec![op(">"), w("file.txt")];
        let mut input = tokens.as_slice();
        let result = io_redirect().parse_next(&mut input);
        assert!(result.is_ok());
        match result.unwrap() {
            crate::ast::IoRedirect::File(n, kind, _) => {
                assert!(n.is_none());
                assert!(matches!(kind, crate::ast::IoFileRedirectKind::Write));
            }
            _ => panic!("Expected File redirect"),
        }
    }

    #[test]
    fn test_io_redirect_with_fd() {
        let tokens = vec![w("2"), op(">"), w("error.log")];
        let mut input = tokens.as_slice();
        let result = io_redirect().parse_next(&mut input);
        assert!(result.is_ok());
        match result.unwrap() {
            crate::ast::IoRedirect::File(n, kind, _) => {
                assert_eq!(n, Some(2));
                assert!(matches!(kind, crate::ast::IoFileRedirectKind::Write));
            }
            _ => panic!("Expected File redirect"),
        }
    }

    #[test]
    fn test_redirect_list_single() {
        let tokens = vec![op(">"), w("out.txt")];
        let mut input = tokens.as_slice();
        let result = redirect_list().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0.len(), 1);
    }

    #[test]
    fn test_redirect_list_multiple() {
        let tokens = vec![op(">"), w("out.txt"), op("<"), w("in.txt")];
        let mut input = tokens.as_slice();
        let result = redirect_list().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0.len(), 2);
    }

    #[test]
    fn test_io_here_basic() {
        let tokens = vec![op("<<"), w("EOF"), w("content here"), w("EOF")];
        let mut input = tokens.as_slice();
        let result = io_here().parse_next(&mut input);
        assert!(result.is_ok());
        let doc = result.unwrap();
        assert!(!doc.remove_tabs);
        assert!(doc.requires_expansion);
        assert_eq!(doc.here_end.value, "EOF");
        assert_eq!(doc.doc.value, "content here");
    }

    #[test]
    fn test_io_here_with_tab_removal() {
        let tokens = vec![op("<<-"), w("END"), w("tabbed content"), w("END")];
        let mut input = tokens.as_slice();
        let result = io_here().parse_next(&mut input);
        assert!(result.is_ok());
        let doc = result.unwrap();
        assert!(doc.remove_tabs);
    }

    #[test]
    fn test_io_here_quoted_tag_no_expansion() {
        let tokens = vec![op("<<"), w("'EOF'"), w("$VAR stays literal"), w("'EOF'")];
        let mut input = tokens.as_slice();
        let result = io_here().parse_next(&mut input);
        assert!(result.is_ok());
        let doc = result.unwrap();
        assert!(!doc.requires_expansion); // quoted tag = no expansion
    }

    #[test]
    fn test_io_here_mismatched_tags_fails() {
        let tokens = vec![op("<<"), w("EOF"), w("content"), w("END")];
        let mut input = tokens.as_slice();
        let result = io_here().parse_next(&mut input);
        assert!(result.is_err());
    }

    // ========================================================================
    // Tier 2 Tests: Assignment Parsers
    // ========================================================================

    #[test]
    fn test_scalar_assignment_simple() {
        let tokens = vec![w("VAR=value")];
        let mut input = tokens.as_slice();
        let result = scalar_assignment_word().parse_next(&mut input);
        assert!(result.is_ok());
        let (assignment, word) = result.unwrap();
        assert!(matches!(
            assignment.name,
            crate::ast::AssignmentName::VariableName(ref n) if n == "VAR"
        ));
        match &assignment.value {
            crate::ast::AssignmentValue::Scalar(w) => assert_eq!(w.value, "value"),
            _ => panic!("Expected scalar value"),
        }
        assert!(!assignment.append);
        assert_eq!(word.value, "VAR=value");
    }

    #[test]
    fn test_scalar_assignment_empty_value() {
        let tokens = vec![w("VAR=")];
        let mut input = tokens.as_slice();
        let result = scalar_assignment_word().parse_next(&mut input);
        assert!(result.is_ok());
        let (assignment, _) = result.unwrap();
        match &assignment.value {
            crate::ast::AssignmentValue::Scalar(w) => assert_eq!(w.value, ""),
            _ => panic!("Expected scalar value"),
        }
    }

    #[test]
    fn test_scalar_assignment_append() {
        let tokens = vec![w("VAR+=more")];
        let mut input = tokens.as_slice();
        let result = scalar_assignment_word().parse_next(&mut input);
        assert!(result.is_ok());
        let (assignment, _) = result.unwrap();
        assert!(matches!(
            assignment.name,
            crate::ast::AssignmentName::VariableName(ref n) if n == "VAR"
        ));
        assert!(assignment.append);
        match &assignment.value {
            crate::ast::AssignmentValue::Scalar(w) => assert_eq!(w.value, "more"),
            _ => panic!("Expected scalar value"),
        }
    }

    #[test]
    fn test_scalar_assignment_array_element() {
        let tokens = vec![w("ARR[0]=first")];
        let mut input = tokens.as_slice();
        let result = scalar_assignment_word().parse_next(&mut input);
        assert!(result.is_ok());
        let (assignment, _) = result.unwrap();
        match &assignment.name {
            crate::ast::AssignmentName::ArrayElementName(name, idx) => {
                assert_eq!(name, "ARR");
                assert_eq!(idx, "0");
            }
            _ => panic!("Expected array element name"),
        }
        match &assignment.value {
            crate::ast::AssignmentValue::Scalar(w) => assert_eq!(w.value, "first"),
            _ => panic!("Expected scalar value"),
        }
    }

    #[test]
    fn test_scalar_assignment_array_element_with_expr() {
        let tokens = vec![w("ARR[i+1]=val")];
        let mut input = tokens.as_slice();
        let result = scalar_assignment_word().parse_next(&mut input);
        assert!(result.is_ok());
        let (assignment, _) = result.unwrap();
        match &assignment.name {
            crate::ast::AssignmentName::ArrayElementName(name, idx) => {
                assert_eq!(name, "ARR");
                assert_eq!(idx, "i+1");
            }
            _ => panic!("Expected array element name"),
        }
    }

    #[test]
    fn test_scalar_assignment_array_element_append() {
        let tokens = vec![w("ARR[0]+=more")];
        let mut input = tokens.as_slice();
        let result = scalar_assignment_word().parse_next(&mut input);
        assert!(result.is_ok());
        let (assignment, _) = result.unwrap();
        assert!(assignment.append);
        match &assignment.name {
            crate::ast::AssignmentName::ArrayElementName(name, idx) => {
                assert_eq!(name, "ARR");
                assert_eq!(idx, "0");
            }
            _ => panic!("Expected array element name"),
        }
    }

    #[test]
    fn test_scalar_assignment_fails_no_equals() {
        let tokens = vec![w("NOEQUALS")];
        let mut input = tokens.as_slice();
        let result = scalar_assignment_word().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_scalar_assignment_fails_invalid_name() {
        let tokens = vec![w("123=value")];
        let mut input = tokens.as_slice();
        let result = scalar_assignment_word().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_scalar_assignment_underscore_name() {
        let tokens = vec![w("_VAR=value")];
        let mut input = tokens.as_slice();
        let result = scalar_assignment_word().parse_next(&mut input);
        assert!(result.is_ok());
        let (assignment, _) = result.unwrap();
        assert!(matches!(
            assignment.name,
            crate::ast::AssignmentName::VariableName(ref n) if n == "_VAR"
        ));
    }

    #[test]
    fn test_array_assignment_simple() {
        let tokens = vec![w("ARR="), op("("), w("a"), w("b"), w("c"), op(")")];
        let mut input = tokens.as_slice();
        let result = array_assignment_word().parse_next(&mut input);
        assert!(result.is_ok());
        let (assignment, word) = result.unwrap();
        assert!(matches!(
            assignment.name,
            crate::ast::AssignmentName::VariableName(ref n) if n == "ARR"
        ));
        match &assignment.value {
            crate::ast::AssignmentValue::Array(elements) => {
                assert_eq!(elements.len(), 3);
                assert_eq!(elements[0].1.value, "a");
                assert_eq!(elements[1].1.value, "b");
                assert_eq!(elements[2].1.value, "c");
            }
            _ => panic!("Expected array value"),
        }
        assert!(!assignment.append);
        assert_eq!(word.value, "ARR=(a b c)");
    }

    #[test]
    fn test_array_assignment_empty() {
        let tokens = vec![w("ARR="), op("("), op(")")];
        let mut input = tokens.as_slice();
        let result = array_assignment_word().parse_next(&mut input);
        assert!(result.is_ok());
        let (assignment, word) = result.unwrap();
        match &assignment.value {
            crate::ast::AssignmentValue::Array(elements) => {
                assert_eq!(elements.len(), 0);
            }
            _ => panic!("Expected array value"),
        }
        assert_eq!(word.value, "ARR=()");
    }

    #[test]
    fn test_array_assignment_single_element() {
        let tokens = vec![w("ARR="), op("("), w("only"), op(")")];
        let mut input = tokens.as_slice();
        let result = array_assignment_word().parse_next(&mut input);
        assert!(result.is_ok());
        let (assignment, _) = result.unwrap();
        match &assignment.value {
            crate::ast::AssignmentValue::Array(elements) => {
                assert_eq!(elements.len(), 1);
                assert_eq!(elements[0].1.value, "only");
            }
            _ => panic!("Expected array value"),
        }
    }

    #[test]
    fn test_array_assignment_fails_invalid_name() {
        let tokens = vec![w("123="), op("("), w("a"), op(")")];
        let mut input = tokens.as_slice();
        let result = array_assignment_word().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_assignment_word_prefers_array() {
        // When both could match, array should be tried first
        let tokens = vec![w("ARR="), op("("), w("x"), op(")")];
        let mut input = tokens.as_slice();
        let result = assignment_word().parse_next(&mut input);
        assert!(result.is_ok());
        let (assignment, _) = result.unwrap();
        assert!(matches!(
            assignment.value,
            crate::ast::AssignmentValue::Array(_)
        ));
    }

    #[test]
    fn test_assignment_word_falls_back_to_scalar() {
        let tokens = vec![w("VAR=value")];
        let mut input = tokens.as_slice();
        let result = assignment_word().parse_next(&mut input);
        assert!(result.is_ok());
        let (assignment, _) = result.unwrap();
        assert!(matches!(
            assignment.value,
            crate::ast::AssignmentValue::Scalar(_)
        ));
    }

    // ========================================================================
    // Tier 3 Tests: Simple Command Parsers
    // ========================================================================

    #[test]
    fn test_any_parses_word() {
        let tokens = vec![w("hello")];
        let mut input = tokens.as_slice();
        let result = any().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_str(), "hello");
        assert!(input.is_empty());
    }

    #[test]
    fn test_any_parses_operator() {
        let tokens = vec![op(";")];
        let mut input = tokens.as_slice();
        let result = any().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_str(), ";");
    }

    #[test]
    fn test_any_fails_on_empty() {
        let tokens: Vec<Token> = vec![];
        let mut input = tokens.as_slice();
        let result = any().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_cmd_name_success() {
        let tokens = vec![w("echo")];
        let mut input = tokens.as_slice();
        let result = cmd_name().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_str(), "echo");
    }

    #[test]
    fn test_cmd_name_fails_on_reserved() {
        let tokens = vec![w("if")];
        let mut input = tokens.as_slice();
        let result = cmd_name().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_cmd_word_success() {
        let tokens = vec![w("ls")];
        let mut input = tokens.as_slice();
        let result = cmd_word().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_str(), "ls");
    }

    #[test]
    fn test_cmd_word_fails_on_assignment() {
        let tokens = vec![w("VAR=value")];
        let mut input = tokens.as_slice();
        let result = cmd_word().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_cmd_word_fails_on_reserved() {
        let tokens = vec![w("while")];
        let mut input = tokens.as_slice();
        let result = cmd_word().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_cmd_prefix_single_assignment() {
        let tokens = vec![w("VAR=val"), w("cmd")];
        let mut input = tokens.as_slice();
        let result = cmd_prefix().parse_next(&mut input);
        assert!(result.is_ok());
        let prefix = result.unwrap();
        assert_eq!(prefix.0.len(), 1);
        assert!(matches!(
            &prefix.0[0],
            crate::ast::CommandPrefixOrSuffixItem::AssignmentWord(_, _)
        ));
        assert_eq!(input.len(), 1); // "cmd" remains
    }

    #[test]
    fn test_cmd_prefix_single_redirect() {
        let tokens = vec![op("<"), w("input.txt"), w("cmd")];
        let mut input = tokens.as_slice();
        let result = cmd_prefix().parse_next(&mut input);
        assert!(result.is_ok());
        let prefix = result.unwrap();
        assert_eq!(prefix.0.len(), 1);
        assert!(matches!(
            &prefix.0[0],
            crate::ast::CommandPrefixOrSuffixItem::IoRedirect(_)
        ));
    }

    #[test]
    fn test_cmd_prefix_multiple_items() {
        let tokens = vec![w("A=1"), w("B=2"), op(">"), w("out"), w("cmd")];
        let mut input = tokens.as_slice();
        let result = cmd_prefix().parse_next(&mut input);
        assert!(result.is_ok());
        let prefix = result.unwrap();
        assert_eq!(prefix.0.len(), 3); // 2 assignments + 1 redirect
        assert_eq!(input.len(), 1); // "cmd" remains
    }

    #[test]
    fn test_cmd_prefix_fails_on_non_prefix() {
        let tokens = vec![w("echo")]; // not an assignment, not a redirect
        let mut input = tokens.as_slice();
        let result = cmd_prefix().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_cmd_suffix_single_word() {
        let tokens = vec![w("arg1")];
        let mut input = tokens.as_slice();
        let result = cmd_suffix().parse_next(&mut input);
        assert!(result.is_ok());
        let suffix = result.unwrap();
        assert_eq!(suffix.0.len(), 1);
        assert!(matches!(
            &suffix.0[0],
            crate::ast::CommandPrefixOrSuffixItem::Word(_)
        ));
    }

    #[test]
    fn test_cmd_suffix_multiple_words() {
        let tokens = vec![w("arg1"), w("arg2"), w("arg3")];
        let mut input = tokens.as_slice();
        let result = cmd_suffix().parse_next(&mut input);
        assert!(result.is_ok());
        let suffix = result.unwrap();
        assert_eq!(suffix.0.len(), 3);
    }

    #[test]
    fn test_cmd_suffix_with_redirect() {
        let tokens = vec![w("arg"), op(">"), w("out.txt")];
        let mut input = tokens.as_slice();
        let result = cmd_suffix().parse_next(&mut input);
        assert!(result.is_ok());
        let suffix = result.unwrap();
        assert_eq!(suffix.0.len(), 2); // word + redirect
    }

    #[test]
    fn test_cmd_suffix_mixed() {
        let tokens = vec![w("arg1"), op("<"), w("in"), w("arg2"), op(">"), w("out")];
        let mut input = tokens.as_slice();
        let result = cmd_suffix().parse_next(&mut input);
        assert!(result.is_ok());
        let suffix = result.unwrap();
        assert_eq!(suffix.0.len(), 4); // arg1 + redirect + arg2 + redirect
    }

    #[test]
    fn test_simple_command_name_only() {
        let tokens = vec![w("ls")];
        let mut input = tokens.as_slice();
        let result = simple_command().parse_next(&mut input);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        assert!(cmd.prefix.is_none());
        assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "ls");
        assert!(cmd.suffix.is_none());
    }

    #[test]
    fn test_simple_command_name_with_args() {
        let tokens = vec![w("echo"), w("hello"), w("world")];
        let mut input = tokens.as_slice();
        let result = simple_command().parse_next(&mut input);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        assert!(cmd.prefix.is_none());
        assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "echo");
        assert!(cmd.suffix.is_some());
        assert_eq!(cmd.suffix.as_ref().unwrap().0.len(), 2);
    }

    #[test]
    fn test_simple_command_with_redirect() {
        let tokens = vec![w("cat"), op("<"), w("file.txt")];
        let mut input = tokens.as_slice();
        let result = simple_command().parse_next(&mut input);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "cat");
        assert!(cmd.suffix.is_some());
    }

    #[test]
    fn test_simple_command_prefix_only() {
        // Just assignments, no command name
        let tokens = vec![w("VAR=value")];
        let mut input = tokens.as_slice();
        let result = simple_command().parse_next(&mut input);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        assert!(cmd.prefix.is_some());
        assert!(cmd.word_or_name.is_none());
        assert!(cmd.suffix.is_none());
    }

    #[test]
    fn test_simple_command_prefix_with_command() {
        let tokens = vec![w("VAR=value"), w("echo"), w("hello")];
        let mut input = tokens.as_slice();
        let result = simple_command().parse_next(&mut input);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        assert!(cmd.prefix.is_some());
        assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "echo");
        assert!(cmd.suffix.is_some());
        assert_eq!(cmd.suffix.as_ref().unwrap().0.len(), 1);
    }

    #[test]
    fn test_simple_command_redirect_prefix() {
        let tokens = vec![op(">"), w("out.txt"), w("echo"), w("hi")];
        let mut input = tokens.as_slice();
        let result = simple_command().parse_next(&mut input);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        assert!(cmd.prefix.is_some());
        assert_eq!(cmd.prefix.as_ref().unwrap().0.len(), 1);
        assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "echo");
    }

    #[test]
    fn test_simple_command_complex() {
        // VAR=x cmd arg1 >out <in
        let tokens = vec![
            w("VAR=x"),
            w("cmd"),
            w("arg1"),
            op(">"),
            w("out"),
            op("<"),
            w("in"),
        ];
        let mut input = tokens.as_slice();
        let result = simple_command().parse_next(&mut input);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        assert!(cmd.prefix.is_some());
        assert_eq!(cmd.word_or_name.as_ref().unwrap().value, "cmd");
        assert!(cmd.suffix.is_some());
        // suffix should have: arg1, >out, <in = 3 items
        assert_eq!(cmd.suffix.as_ref().unwrap().0.len(), 3);
    }

    #[test]
    fn test_simple_command_fails_on_reserved_word() {
        let tokens = vec![w("if")];
        let mut input = tokens.as_slice();
        let result = simple_command().parse_next(&mut input);
        assert!(result.is_err());
    }

    // ========================================================================
    // Tier 3 Tests: Special Commands (arithmetic, extended test)
    // ========================================================================

    #[test]
    fn test_arithmetic_expression_simple() {
        // Simulates content between (( and ))
        let tokens = vec![w("1"), op("+"), w("2"), op(")"), op(")")];
        let mut input = tokens.as_slice();
        let result = arithmetic_expression().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().value, "1+2");
        assert_eq!(input.len(), 2); // )) remains
    }

    #[test]
    fn test_arithmetic_expression_with_spaces() {
        let tokens = vec![w("a"), w("b"), w("c"), op(")"), op(")")];
        let mut input = tokens.as_slice();
        let result = arithmetic_expression().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().value, "a b c");
    }

    #[test]
    fn test_arithmetic_expression_nested_parens() {
        // ((1 + (2 * 3)))
        let tokens = vec![
            w("1"),
            op("+"),
            op("("),
            w("2"),
            op("*"),
            w("3"),
            op(")"),
            op(")"),
            op(")"),
        ];
        let mut input = tokens.as_slice();
        let result = arithmetic_expression().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().value, "1+(2*3)");
        assert_eq!(input.len(), 2); // )) remains
    }

    #[test]
    fn test_arithmetic_expression_stops_at_semicolon() {
        // For arithmetic for loops: for ((i=0; i<10; i++))
        let tokens = vec![w("i"), op("="), w("0"), op(";"), w("more")];
        let mut input = tokens.as_slice();
        let result = arithmetic_expression().parse_next(&mut input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().value, "i=0");
        assert_eq!(input.len(), 2); // ; and more remain
    }

    #[test]
    fn test_arithmetic_command_simple() {
        let tokens = vec![op("("), op("("), w("1"), op("+"), w("2"), op(")"), op(")")];
        let mut input = tokens.as_slice();
        let result = arithmetic_command().parse_next(&mut input);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        assert_eq!(cmd.expr.value, "1+2");
        assert!(input.is_empty());
    }

    #[test]
    fn test_arithmetic_command_variable() {
        let tokens = vec![op("("), op("("), w("x"), op("++"), op(")"), op(")")];
        let mut input = tokens.as_slice();
        let result = arithmetic_command().parse_next(&mut input);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        assert_eq!(cmd.expr.value, "x++");
    }

    #[test]
    fn test_arithmetic_command_fails_single_paren() {
        let tokens = vec![op("("), w("x"), op(")")];
        let mut input = tokens.as_slice();
        let result = arithmetic_command().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_extended_test_unary_file_exists() {
        let tokens = vec![w("[["), w("-e"), w("/path/to/file"), w("]]")];
        let mut input = tokens.as_slice();
        let result = extended_test_command().parse_next(&mut input);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        match &cmd.expr {
            crate::ast::ExtendedTestExpr::UnaryTest(pred, operand) => {
                assert!(matches!(pred, crate::ast::UnaryPredicate::FileExists));
                assert_eq!(operand.value, "/path/to/file");
            }
            _ => panic!("Expected UnaryTest"),
        }
    }

    #[test]
    fn test_extended_test_unary_string_zero_length() {
        let tokens = vec![w("[["), w("-z"), w("$VAR"), w("]]")];
        let mut input = tokens.as_slice();
        let result = extended_test_command().parse_next(&mut input);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        match &cmd.expr {
            crate::ast::ExtendedTestExpr::UnaryTest(pred, _) => {
                assert!(matches!(
                    pred,
                    crate::ast::UnaryPredicate::StringHasZeroLength
                ));
            }
            _ => panic!("Expected UnaryTest"),
        }
    }

    #[test]
    fn test_extended_test_binary_string_equals() {
        let tokens = vec![w("[["), w("$a"), w("="), w("$b"), w("]]")];
        let mut input = tokens.as_slice();
        let result = extended_test_command().parse_next(&mut input);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        match &cmd.expr {
            crate::ast::ExtendedTestExpr::BinaryTest(pred, left, right) => {
                assert!(matches!(
                    pred,
                    crate::ast::BinaryPredicate::StringExactlyMatchesString
                ));
                assert_eq!(left.value, "$a");
                assert_eq!(right.value, "$b");
            }
            _ => panic!("Expected BinaryTest"),
        }
    }

    #[test]
    fn test_extended_test_binary_arithmetic() {
        let tokens = vec![w("[["), w("$x"), w("-gt"), w("10"), w("]]")];
        let mut input = tokens.as_slice();
        let result = extended_test_command().parse_next(&mut input);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        match &cmd.expr {
            crate::ast::ExtendedTestExpr::BinaryTest(pred, _, _) => {
                assert!(matches!(
                    pred,
                    crate::ast::BinaryPredicate::ArithmeticGreaterThan
                ));
            }
            _ => panic!("Expected BinaryTest"),
        }
    }

    #[test]
    fn test_extended_test_single_word_fallback() {
        // Single word becomes StringHasNonZeroLength test
        let tokens = vec![w("[["), w("$VAR"), w("]]")];
        let mut input = tokens.as_slice();
        let result = extended_test_command().parse_next(&mut input);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        match &cmd.expr {
            crate::ast::ExtendedTestExpr::UnaryTest(pred, operand) => {
                assert!(matches!(
                    pred,
                    crate::ast::UnaryPredicate::StringHasNonZeroLength
                ));
                assert_eq!(operand.value, "$VAR");
            }
            _ => panic!("Expected UnaryTest"),
        }
    }

    #[test]
    fn test_extended_test_fails_without_closing() {
        let tokens = vec![w("[["), w("-e"), w("file")];
        let mut input = tokens.as_slice();
        let result = extended_test_command().parse_next(&mut input);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_unary_operator_file_tests() {
        assert!(matches!(
            parse_unary_operator("-d"),
            Some(crate::ast::UnaryPredicate::FileExistsAndIsDir)
        ));
        assert!(matches!(
            parse_unary_operator("-f"),
            Some(crate::ast::UnaryPredicate::FileExistsAndIsRegularFile)
        ));
        assert!(matches!(
            parse_unary_operator("-r"),
            Some(crate::ast::UnaryPredicate::FileExistsAndIsReadable)
        ));
        assert!(matches!(
            parse_unary_operator("-w"),
            Some(crate::ast::UnaryPredicate::FileExistsAndIsWritable)
        ));
        assert!(matches!(
            parse_unary_operator("-x"),
            Some(crate::ast::UnaryPredicate::FileExistsAndIsExecutable)
        ));
    }

    #[test]
    fn test_parse_unary_operator_string_tests() {
        assert!(matches!(
            parse_unary_operator("-z"),
            Some(crate::ast::UnaryPredicate::StringHasZeroLength)
        ));
        assert!(matches!(
            parse_unary_operator("-n"),
            Some(crate::ast::UnaryPredicate::StringHasNonZeroLength)
        ));
    }

    #[test]
    fn test_parse_binary_operator_string_tests() {
        assert!(matches!(
            parse_binary_operator("="),
            Some(crate::ast::BinaryPredicate::StringExactlyMatchesString)
        ));
        assert!(matches!(
            parse_binary_operator("=="),
            Some(crate::ast::BinaryPredicate::StringExactlyMatchesString)
        ));
        assert!(matches!(
            parse_binary_operator("!="),
            Some(crate::ast::BinaryPredicate::StringDoesNotExactlyMatchString)
        ));
    }

    #[test]
    fn test_parse_binary_operator_arithmetic_tests() {
        assert!(matches!(
            parse_binary_operator("-eq"),
            Some(crate::ast::BinaryPredicate::ArithmeticEqualTo)
        ));
        assert!(matches!(
            parse_binary_operator("-ne"),
            Some(crate::ast::BinaryPredicate::ArithmeticNotEqualTo)
        ));
        assert!(matches!(
            parse_binary_operator("-lt"),
            Some(crate::ast::BinaryPredicate::ArithmeticLessThan)
        ));
        assert!(matches!(
            parse_binary_operator("-le"),
            Some(crate::ast::BinaryPredicate::ArithmeticLessThanOrEqualTo)
        ));
        assert!(matches!(
            parse_binary_operator("-gt"),
            Some(crate::ast::BinaryPredicate::ArithmeticGreaterThan)
        ));
        assert!(matches!(
            parse_binary_operator("-ge"),
            Some(crate::ast::BinaryPredicate::ArithmeticGreaterThanOrEqualTo)
        ));
    }

    // ========================================================================
    // Tier 4 Tests: Compound Commands (with StatefulStream)
    // ========================================================================

    fn make_ctx() -> (crate::parser::ParserOptions, crate::parser::SourceInfo) {
        let opts = crate::parser::ParserOptions::default();
        let source_info = crate::parser::SourceInfo::default();
        (opts, source_info)
    }

    fn make_stateful_stream<'a>(
        tokens: &'a [Token],
        opts: &'a crate::parser::ParserOptions,
        source_info: &'a crate::parser::SourceInfo,
    ) -> StatefulStream<'a> {
        make_stream(
            tokens,
            ParserContext {
                options: opts,
                source_info,
            },
        )
    }

    #[test]
    fn test_brace_group_simple() {
        let (opts, source_info) = make_ctx();
        let tokens = vec![w("{"), w("echo"), w("hello"), op(";"), w("}")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = brace_group().parse_next(&mut input);
        assert!(result.is_ok());
        let bg = result.unwrap();
        assert_eq!(bg.list.0.len(), 1);
    }

    #[test]
    fn test_subshell_simple() {
        let (opts, source_info) = make_ctx();
        let tokens = vec![op("("), w("echo"), w("hello"), op(";"), op(")")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = subshell().parse_next(&mut input);
        assert!(result.is_ok());
        let sub = result.unwrap();
        assert_eq!(sub.list.0.len(), 1);
    }

    #[test]
    fn test_do_group_simple() {
        let (opts, source_info) = make_ctx();
        let tokens = vec![w("do"), w("echo"), w("hi"), op(";"), w("done")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = do_group().parse_next(&mut input);
        assert!(result.is_ok());
        let dg = result.unwrap();
        assert_eq!(dg.list.0.len(), 1);
    }

    #[test]
    fn test_for_clause_simple() {
        let (opts, source_info) = make_ctx();
        // for i in a b c; do echo $i; done
        let tokens = vec![
            w("for"),
            w("i"),
            w("in"),
            w("a"),
            w("b"),
            w("c"),
            op(";"),
            w("do"),
            w("echo"),
            w("$i"),
            op(";"),
            w("done"),
        ];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = for_clause().parse_next(&mut input);
        assert!(result.is_ok());
        let fc = result.unwrap();
        assert_eq!(fc.variable_name, "i");
        assert!(fc.values.is_some());
        assert_eq!(fc.values.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn test_for_clause_no_in() {
        let (opts, source_info) = make_ctx();
        // for i; do echo $i; done (iterates over positional params)
        let tokens = vec![
            w("for"),
            w("i"),
            op(";"),
            w("do"),
            w("echo"),
            w("$i"),
            op(";"),
            w("done"),
        ];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = for_clause().parse_next(&mut input);
        assert!(result.is_ok());
        let fc = result.unwrap();
        assert_eq!(fc.variable_name, "i");
        assert!(fc.values.is_none());
    }

    #[test]
    fn test_while_clause_simple() {
        let (opts, source_info) = make_ctx();
        // while true; do echo loop; done
        let tokens = vec![
            w("while"),
            w("true"),
            op(";"),
            w("do"),
            w("echo"),
            w("loop"),
            op(";"),
            w("done"),
        ];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = while_clause().parse_next(&mut input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_until_clause_simple() {
        let (opts, source_info) = make_ctx();
        // until false; do echo loop; done
        let tokens = vec![
            w("until"),
            w("false"),
            op(";"),
            w("do"),
            w("echo"),
            w("loop"),
            op(";"),
            w("done"),
        ];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = until_clause().parse_next(&mut input);
        assert!(result.is_ok());
    }

    #[test]
    fn test_if_clause_simple() {
        let (opts, source_info) = make_ctx();
        // if true; then echo yes; fi
        let tokens = vec![
            w("if"),
            w("true"),
            op(";"),
            w("then"),
            w("echo"),
            w("yes"),
            op(";"),
            w("fi"),
        ];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = if_clause().parse_next(&mut input);
        assert!(result.is_ok());
        let ic = result.unwrap();
        assert!(ic.elses.is_none());
    }

    #[test]
    fn test_if_clause_with_else() {
        let (opts, source_info) = make_ctx();
        // if true; then echo yes; else echo no; fi
        let tokens = vec![
            w("if"),
            w("true"),
            op(";"),
            w("then"),
            w("echo"),
            w("yes"),
            op(";"),
            w("else"),
            w("echo"),
            w("no"),
            op(";"),
            w("fi"),
        ];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = if_clause().parse_next(&mut input);
        assert!(result.is_ok());
        let ic = result.unwrap();
        assert!(ic.elses.is_some());
        assert_eq!(ic.elses.as_ref().unwrap().len(), 1);
        assert!(ic.elses.as_ref().unwrap()[0].condition.is_none()); // else, not elif
    }

    #[test]
    fn test_if_clause_with_elif() {
        let (opts, source_info) = make_ctx();
        // if true; then echo a; elif false; then echo b; fi
        let tokens = vec![
            w("if"),
            w("true"),
            op(";"),
            w("then"),
            w("echo"),
            w("a"),
            op(";"),
            w("elif"),
            w("false"),
            op(";"),
            w("then"),
            w("echo"),
            w("b"),
            op(";"),
            w("fi"),
        ];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = if_clause().parse_next(&mut input);
        assert!(result.is_ok());
        let ic = result.unwrap();
        assert!(ic.elses.is_some());
        assert_eq!(ic.elses.as_ref().unwrap().len(), 1);
        assert!(ic.elses.as_ref().unwrap()[0].condition.is_some()); // elif has condition
    }

    #[test]
    fn test_case_clause_simple() {
        let (opts, source_info) = make_ctx();
        // case $x in a) echo a;; esac
        let tokens = vec![
            w("case"),
            w("$x"),
            w("in"),
            w("a"),
            op(")"),
            w("echo"),
            w("a"),
            op(";;"),
            w("esac"),
        ];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = case_clause().parse_next(&mut input);
        assert!(result.is_ok());
        let cc = result.unwrap();
        assert_eq!(cc.value.value, "$x");
        assert_eq!(cc.cases.len(), 1);
        assert_eq!(cc.cases[0].patterns.len(), 1);
        assert_eq!(cc.cases[0].patterns[0].value, "a");
    }

    #[test]
    fn test_case_clause_multiple_patterns() {
        let (opts, source_info) = make_ctx();
        // case $x in a|b|c) echo match;; esac
        let tokens = vec![
            w("case"),
            w("$x"),
            w("in"),
            w("a"),
            op("|"),
            w("b"),
            op("|"),
            w("c"),
            op(")"),
            w("echo"),
            w("match"),
            op(";;"),
            w("esac"),
        ];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = case_clause().parse_next(&mut input);
        assert!(result.is_ok());
        let cc = result.unwrap();
        assert_eq!(cc.cases[0].patterns.len(), 3);
    }

    #[test]
    fn test_function_definition_with_keyword() {
        let (opts, source_info) = make_ctx();
        // function foo { echo hello; }
        let tokens = vec![
            w("function"),
            w("foo"),
            w("{"),
            w("echo"),
            w("hello"),
            op(";"),
            w("}"),
        ];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = function_definition().parse_next(&mut input);
        assert!(result.is_ok());
        let fd = result.unwrap();
        assert_eq!(fd.fname.value, "foo");
    }

    #[test]
    fn test_function_definition_with_parens() {
        let (opts, source_info) = make_ctx();
        // foo() { echo hello; }
        let tokens = vec![
            w("foo"),
            op("("),
            op(")"),
            w("{"),
            w("echo"),
            w("hello"),
            op(";"),
            w("}"),
        ];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = function_definition().parse_next(&mut input);
        assert!(result.is_ok());
        let fd = result.unwrap();
        assert_eq!(fd.fname.value, "foo");
    }

    #[test]
    fn test_pipeline_simple() {
        let (opts, source_info) = make_ctx();
        // echo hello
        let tokens = vec![w("echo"), w("hello")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = pipeline().parse_next(&mut input);
        assert!(result.is_ok());
        let pl = result.unwrap();
        assert!(!pl.bang);
        assert_eq!(pl.seq.len(), 1);
    }

    #[test]
    fn test_pipeline_with_pipe() {
        let (opts, source_info) = make_ctx();
        // echo hello | cat
        let tokens = vec![w("echo"), w("hello"), op("|"), w("cat")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = pipeline().parse_next(&mut input);
        assert!(result.is_ok());
        let pl = result.unwrap();
        assert_eq!(pl.seq.len(), 2);
    }

    #[test]
    fn test_pipeline_negated() {
        let (opts, source_info) = make_ctx();
        // ! echo hello
        // Note: ! is a word, not an operator, in shell tokenization
        let tokens = vec![w("!"), w("echo"), w("hello")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = pipeline().parse_next(&mut input);
        assert!(result.is_ok());
        let pl = result.unwrap();
        assert!(pl.bang);
    }

    #[test]
    fn test_and_or_list_simple() {
        let (opts, source_info) = make_ctx();
        // echo hello
        let tokens = vec![w("echo"), w("hello")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = and_or().parse_next(&mut input);
        assert!(result.is_ok());
        let ao = result.unwrap();
        assert!(ao.additional.is_empty());
    }

    #[test]
    fn test_and_or_list_with_and() {
        let (opts, source_info) = make_ctx();
        // true && echo yes
        let tokens = vec![w("true"), op("&&"), w("echo"), w("yes")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = and_or().parse_next(&mut input);
        assert!(result.is_ok());
        let ao = result.unwrap();
        assert_eq!(ao.additional.len(), 1);
        assert!(matches!(ao.additional[0], crate::ast::AndOr::And(_)));
    }

    #[test]
    fn test_and_or_list_with_or() {
        let (opts, source_info) = make_ctx();
        // false || echo fallback
        let tokens = vec![w("false"), op("||"), w("echo"), w("fallback")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = and_or().parse_next(&mut input);
        assert!(result.is_ok());
        let ao = result.unwrap();
        assert_eq!(ao.additional.len(), 1);
        assert!(matches!(ao.additional[0], crate::ast::AndOr::Or(_)));
    }

    #[test]
    fn test_compound_command_brace_group() {
        let (opts, source_info) = make_ctx();
        let tokens = vec![w("{"), w("echo"), w("hi"), op(";"), w("}")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = compound_command().parse_next(&mut input);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            crate::ast::CompoundCommand::BraceGroup(_)
        ));
    }

    #[test]
    fn test_compound_command_subshell() {
        let (opts, source_info) = make_ctx();
        let tokens = vec![op("("), w("echo"), w("hi"), op(";"), op(")")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = compound_command().parse_next(&mut input);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            crate::ast::CompoundCommand::Subshell(_)
        ));
    }

    #[test]
    fn test_compound_command_if() {
        let (opts, source_info) = make_ctx();
        let tokens = vec![
            w("if"),
            w("true"),
            op(";"),
            w("then"),
            w("echo"),
            w("yes"),
            op(";"),
            w("fi"),
        ];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = compound_command().parse_next(&mut input);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            crate::ast::CompoundCommand::IfClause(_)
        ));
    }

    // ========================================================================
    // Tier 5 Tests: Top-Level Parsers
    // ========================================================================

    #[test]
    fn test_pipeline_timed() {
        // time echo hello
        let tokens = vec![w("time"), w("echo"), w("hello")];
        let mut input = tokens.as_slice();
        let result = pipeline_timed().parse_next(&mut input);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            crate::ast::PipelineTimed::Timed(_)
        ));
    }

    #[test]
    fn test_pipeline_timed_with_posix() {
        // time -p echo hello
        let tokens = vec![w("time"), w("-p"), w("echo"), w("hello")];
        let mut input = tokens.as_slice();
        let result = pipeline_timed().parse_next(&mut input);
        assert!(result.is_ok());
        assert!(matches!(
            result.unwrap(),
            crate::ast::PipelineTimed::TimedWithPosixOutput(_)
        ));
    }

    #[test]
    fn test_pipe_sequence_single() {
        let (opts, source_info) = make_ctx();
        // echo hello
        let tokens = vec![w("echo"), w("hello")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = pipe_sequence().parse_next(&mut input);
        assert!(result.is_ok());
        let cmds = result.unwrap();
        assert_eq!(cmds.len(), 1);
    }

    #[test]
    fn test_pipe_sequence_multiple() {
        let (opts, source_info) = make_ctx();
        // echo hello | cat
        let tokens = vec![w("echo"), w("hello"), op("|"), w("cat")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = pipe_sequence().parse_next(&mut input);
        assert!(result.is_ok());
        let cmds = result.unwrap();
        assert_eq!(cmds.len(), 2);
    }

    #[test]
    fn test_pipeline_timed_full() {
        let (opts, source_info) = make_ctx();
        // time echo hello
        let tokens = vec![w("time"), w("echo"), w("hello")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = pipeline().parse_next(&mut input);
        assert!(result.is_ok());
        let pl = result.unwrap();
        assert!(pl.timed.is_some());
        assert!(!pl.bang);
    }

    #[test]
    fn test_pipeline_negated_timed() {
        let (opts, source_info) = make_ctx();
        // time ! echo hello
        // Note: ! is a word, not an operator in shell tokenization
        let tokens = vec![w("time"), w("!"), w("echo"), w("hello")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = pipeline().parse_next(&mut input);
        assert!(result.is_ok());
        let pl = result.unwrap();
        assert!(pl.timed.is_some());
        assert!(pl.bang);
    }

    #[test]
    fn test_complete_command_single() {
        let (opts, source_info) = make_ctx();
        // echo hello
        let tokens = vec![w("echo"), w("hello")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = complete_command().parse_next(&mut input);
        assert!(result.is_ok());
        let list = result.unwrap();
        assert_eq!(list.0.len(), 1);
    }

    #[test]
    fn test_complete_command_with_semicolon() {
        let (opts, source_info) = make_ctx();
        // echo hello; echo world
        let tokens = vec![w("echo"), w("hello"), op(";"), w("echo"), w("world")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = complete_command().parse_next(&mut input);
        assert!(result.is_ok());
        let list = result.unwrap();
        assert_eq!(list.0.len(), 2);
    }

    #[test]
    fn test_complete_command_with_background() {
        let (opts, source_info) = make_ctx();
        // sleep 10 &
        let tokens = vec![w("sleep"), w("10"), op("&")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = complete_command().parse_next(&mut input);
        assert!(result.is_ok());
        let list = result.unwrap();
        assert_eq!(list.0.len(), 1);
        assert!(matches!(list.0[0].1, SeparatorOperator::Async));
    }

    #[test]
    fn test_complete_commands_single() {
        let (opts, source_info) = make_ctx();
        // echo hello
        let tokens = vec![w("echo"), w("hello")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = complete_commands().parse_next(&mut input);
        assert!(result.is_ok());
        let cmds = result.unwrap();
        assert_eq!(cmds.len(), 1);
    }

    #[test]
    fn test_complete_commands_multiple() {
        let (opts, source_info) = make_ctx();
        // echo hello \n echo world
        let tokens = vec![w("echo"), w("hello"), op("\n"), w("echo"), w("world")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = complete_commands().parse_next(&mut input);
        assert!(result.is_ok());
        let cmds = result.unwrap();
        assert_eq!(cmds.len(), 2);
    }

    #[test]
    fn test_program_empty() {
        let (opts, source_info) = make_ctx();
        let tokens: Vec<Token> = vec![];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = program().parse_next(&mut input);
        assert!(result.is_ok());
        let prog = result.unwrap();
        assert!(prog.complete_commands.is_empty());
    }

    #[test]
    fn test_program_single_command() {
        let (opts, source_info) = make_ctx();
        // echo hello
        let tokens = vec![w("echo"), w("hello")];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = program().parse_next(&mut input);
        assert!(result.is_ok());
        let prog = result.unwrap();
        assert_eq!(prog.complete_commands.len(), 1);
    }

    #[test]
    fn test_program_multiple_commands() {
        let (opts, source_info) = make_ctx();
        // \n echo hello \n echo world \n
        let tokens = vec![
            op("\n"),
            w("echo"),
            w("hello"),
            op("\n"),
            w("echo"),
            w("world"),
            op("\n"),
        ];
        let mut input = make_stateful_stream(&tokens, &opts, &source_info);
        let result = program().parse_next(&mut input);
        assert!(result.is_ok());
        let prog = result.unwrap();
        assert_eq!(prog.complete_commands.len(), 2);
    }

    #[test]
    fn test_parse_program_api() {
        let opts = ParserOptions::default();
        let source_info = SourceInfo::default();
        // echo hello; echo world
        let tokens = vec![w("echo"), w("hello"), op(";"), w("echo"), w("world")];
        let result = parse_program(&tokens, &opts, &source_info);
        assert!(result.is_ok());
        let prog = result.unwrap();
        assert_eq!(prog.complete_commands.len(), 1);
        // Complete command has 2 and_or items
        assert_eq!(prog.complete_commands[0].0.len(), 2);
    }

    #[test]
    fn test_parse_program_with_compound_commands() {
        let opts = ParserOptions::default();
        let source_info = SourceInfo::default();
        // if true; then echo yes; fi
        let tokens = vec![
            w("if"),
            w("true"),
            op(";"),
            w("then"),
            w("echo"),
            w("yes"),
            op(";"),
            w("fi"),
        ];
        let result = parse_program(&tokens, &opts, &source_info);
        assert!(result.is_ok());
        let prog = result.unwrap();
        assert_eq!(prog.complete_commands.len(), 1);
    }

    #[test]
    fn test_parse_program_with_extended_test() {
        let opts = ParserOptions::default();
        let source_info = SourceInfo::default();
        // [[ -f /etc/passwd ]]
        let tokens = vec![w("[["), w("-f"), w("/etc/passwd"), w("]]")];
        let result = parse_program(&tokens, &opts, &source_info);
        assert!(result.is_ok());
        let prog = result.unwrap();
        assert_eq!(prog.complete_commands.len(), 1);
        // Verify it's an ExtendedTest command
        let cmd = &prog.complete_commands[0].0[0].0.first;
        assert_eq!(cmd.seq.len(), 1);
        assert!(matches!(cmd.seq[0], crate::ast::Command::ExtendedTest(..)));
    }

    #[test]
    fn test_command_extended_test_enabled_in_bash_mode() {
        let opts = ParserOptions::default();
        let source_info = SourceInfo::default();
        // [[ -f /etc/passwd ]] - in bash mode, should parse as ExtendedTest
        let tokens = vec![w("[["), w("-f"), w("/etc/passwd"), w("]]")];
        let ctx = ParserContext {
            options: &opts,
            source_info: &source_info,
        };
        let mut input = make_stream(&tokens, ctx);
        let result = command().parse_next(&mut input);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        // Should be an ExtendedTest command
        assert!(matches!(cmd, crate::ast::Command::ExtendedTest(..)));
    }
}
