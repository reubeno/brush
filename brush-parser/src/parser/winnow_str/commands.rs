use winnow::combinator::{fail, trace};
use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::token::take_while;

use crate::ast;

use super::compound::{compound_command, process_substitution};
use super::helpers::{array_spaces, parse_balanced_delimiters, peek_char, peek_first_word, spaces};
use super::position::PositionTracker;
use super::redirections::{io_redirect, optional_redirects};
use super::types::{ParseContext, StrStream};
use super::words::{non_reserved_word, word_as_ast, word_part};

// ============================================================================
// Commands
// ============================================================================

/// Parse an array element value (handles quotes properly, stops at ')' or whitespace)
fn array_element_value<'a>(
    ctx: &'a ParseContext<'a>,
) -> impl ModalParser<StrStream<'a>, String, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let mut value = String::new();

        loop {
            // Check if we should stop (at ')' or unquoted whitespace)
            let Ok(ch) = peek_char().parse_next(input) else {
                break; // EOF
            };

            if ch == ')' || ch.is_whitespace() {
                break;
            }

            // Parse the next word part
            let part = word_part(ctx, value.chars().last()).parse_next(input)?;
            value.push_str(&part);
        }

        if value.is_empty() {
            return fail.parse_next(input);
        }

        Ok(value)
    }
}

/// Parse a `[index]` subscript and return the index, without the brackets.
fn subscript<'a>() -> impl ModalParser<StrStream<'a>, &'a str, ContextError> {
    parse_balanced_delimiters("[", Some('['), ']', 1).map(|s: &'a str| {
        s.strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or(s)
    })
}

/// Parse an array element: either "value" or "[index]=value"
fn array_element<'a>(
    ctx: &'a ParseContext<'a>,
) -> impl ModalParser<StrStream<'a>, (Option<ast::Word>, ast::Word), ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Skip whitespace before element (newlines are whitespace inside arrays)
        array_spaces().parse_next(input)?;

        // Try to parse indexed element: [index]=value
        let checkpoint = input.checkpoint();
        let has_bracket: ModalResult<char> = winnow::combinator::peek('[').parse_next(input);
        let has_bracket = has_bracket.is_ok();

        if has_bracket {
            let index_str = subscript().parse_next(input)?;
            if winnow::combinator::opt('=').parse_next(input)?.is_some() {
                // Parse value using proper word parsing that handles quotes
                let value_str = winnow::combinator::opt(array_element_value(ctx))
                    .parse_next(input)?
                    .unwrap_or_default();

                return Ok((Some(ast::Word::new(index_str)), ast::Word::new(&value_str)));
            }
        }

        // Reset and try simple value
        input.reset(&checkpoint);

        // Parse simple value using proper word parsing that handles quotes
        let value_str = array_element_value(ctx).parse_next(input)?;

        Ok((None, ast::Word::new(&value_str)))
    }
}

/// Parse the elements of an array literal and its `)`, the `(` already consumed. Returns the
/// elements, with the whole assignment word as written: `name=(...)` or `name+=(...)`.
fn array_literal<'a>(
    ctx: &'a ParseContext<'a>,
    var_name: &'a str,
    append: bool,
) -> impl ModalParser<StrStream<'a>, (Vec<(Option<ast::Word>, ast::Word)>, String), ContextError> + 'a
{
    move |input: &mut StrStream<'a>| {
        let mut elements = Vec::new();
        let mut full_word = String::with_capacity(var_name.len() + 16);
        full_word.push_str(var_name);
        if append {
            full_word.push_str("+=(");
        } else {
            full_word.push_str("=(");
        }

        loop {
            // Inside array literals, newlines act as whitespace separators
            // (just like spaces/tabs). Consume all whitespace including newlines.
            array_spaces().parse_next(input)?;

            // Check for closing paren
            if winnow::combinator::opt(')').parse_next(input)?.is_some() {
                full_word.push(')');
                break;
            }

            // Parse element
            let elem = array_element(ctx).parse_next(input)?;

            // Add to full_word
            if !elements.is_empty() {
                full_word.push(' ');
            }
            if let Some(ref index) = elem.0 {
                full_word.push('[');
                full_word.push_str(&index.value);
                full_word.push_str("]=");
            }
            full_word.push_str(&elem.1.value);

            elements.push(elem);
        }

        Ok((elements, full_word))
    }
}

/// Parse an assignment word (VAR=value or VAR+=value or VAR[idx]=value or VAR=(array elements))
/// Returns (Assignment, original word as `ast::Word`)
fn assignment_word<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, (ast::Assignment, ast::Word), ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        // Parse variable name (must start with letter or underscore)
        let var_name = (
            winnow::token::one_of(|c: char| c.is_ascii_alphabetic() || c == '_'),
            winnow::token::take_while(0.., |c: char| c.is_ascii_alphanumeric() || c == '_'),
        )
            .take()
            .parse_next(input)?;

        // Check for array element syntax: var[index]
        let peek_bracket: ModalResult<char> = winnow::combinator::peek('[').parse_next(input);
        let array_index = if peek_bracket.is_ok() {
            Some(subscript().parse_next(input)?.to_string())
        } else {
            None
        };

        // Check for optional '+' (append assignment)
        let append = winnow::combinator::opt('+').parse_next(input)?.is_some();

        // Must have '='
        '='.parse_next(input)?;

        // Check if it's an array assignment
        let checkpoint = input.checkpoint();
        let has_paren = winnow::combinator::opt('(').parse_next(input)?.is_some();

        if has_paren {
            let (elements, full_word) = array_literal(ctx, var_name, append).parse_next(input)?;

            let end_offset = tracker.offset_from_locating(input);
            let loc = tracker.range_to_span(start_offset..end_offset);

            let assignment = ast::Assignment {
                name: ast::AssignmentName::VariableName(var_name.to_string()),
                value: ast::AssignmentValue::Array(elements),
                append,
                loc,
            };

            return Ok((assignment, ast::Word::new(&full_word)));
        }

        // Not an array, reset and parse scalar value
        input.reset(&checkpoint);

        // Parse the value using proper word parsing that handles quotes, escapes, etc.
        // The value can be empty (e.g., x=), so use opt
        let value_word = winnow::combinator::opt(word_as_ast(ctx, tracker)).parse_next(input)?;
        let value_str = value_word.as_ref().map_or("", |w| w.value.as_str());

        // Construct the full assignment word for AST
        let mut full_word = String::with_capacity(var_name.len() + value_str.len() + 10);
        full_word.push_str(var_name);
        if let Some(ref idx) = array_index {
            full_word.push('[');
            full_word.push_str(idx);
            full_word.push(']');
        }
        if append {
            full_word.push_str("+=");
        } else {
            full_word.push('=');
        }
        full_word.push_str(value_str);

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        // Use ArrayElementName if we have an index, otherwise VariableName
        let name = if let Some(idx) = array_index {
            ast::AssignmentName::ArrayElementName(var_name.to_string(), idx)
        } else {
            ast::AssignmentName::VariableName(var_name.to_string())
        };

        let assignment = ast::Assignment {
            name,
            value: ast::AssignmentValue::Scalar(ast::Word::new(value_str)),
            append,
            loc,
        };

        let word = ast::Word::new(&full_word);

        Ok((assignment, word))
    }
}

/// Parse `cmd_prefix` (assignments and redirects before command name)
/// Corresponds to: peg.rs `cmd_prefix()`
pub(super) fn cmd_prefix<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::CommandPrefix, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        winnow::combinator::repeat::<_, _, Vec<_>, _, _>(
            1..,
            winnow::combinator::terminated(
                winnow::combinator::alt((
                    io_redirect(ctx, tracker).map(ast::CommandPrefixOrSuffixItem::IoRedirect),
                    assignment_word(ctx, tracker).map(|(assignment, word)| {
                        ast::CommandPrefixOrSuffixItem::AssignmentWord(assignment, word)
                    }),
                )),
                spaces(),
            ),
        )
        .map(ast::CommandPrefix)
        .parse_next(input)
    }
}

/// Parse a single suffix item (word, redirect, process substitution, or assignment).
fn single_suffix_item<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::CommandPrefixOrSuffixItem, ContextError> + 'a {
    winnow::combinator::alt((
        io_redirect(ctx, tracker).map(ast::CommandPrefixOrSuffixItem::IoRedirect),
        process_substitution(ctx, tracker)
            .map(|(kind, cmd)| ast::CommandPrefixOrSuffixItem::ProcessSubstitution(kind, cmd)),
        assignment_word(ctx, tracker).map(|(assignment, word)| {
            ast::CommandPrefixOrSuffixItem::AssignmentWord(assignment, word)
        }),
        word_as_ast(ctx, tracker).map(ast::CommandPrefixOrSuffixItem::Word),
    ))
}

/// Parse `cmd_suffix` (arguments and redirections).
///
/// Now supports words, redirections, and process substitutions.
/// Handles multiple here-docs on the same line properly.
pub(super) fn cmd_suffix<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::CommandSuffix, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Check what's next: space, redirect, or something else
        let Ok(ch) = peek_char().parse_next(input) else {
            return fail.parse_next(input);
        };

        // If there's space, consume it and we can parse any suffix item
        // If there's no space but we see a redirect char or digit, only parse redirects
        // Otherwise, no suffix (backtrack)
        let can_parse_words = if ch == ' ' || ch == '\t' || ch == '\n' {
            spaces().parse_next(input)?;
            true
        } else if ch == '<' || ch == '>' || ch.is_ascii_digit() {
            // No space, but can parse redirects without space
            false
        } else {
            return fail.parse_next(input);
        };

        let mut all_items: Vec<ast::CommandPrefixOrSuffixItem> = Vec::new();

        loop {
            if peek_char().parse_next(input).is_err() {
                break;
            }

            // If we can't parse words (no leading space), only try redirects
            let item = if can_parse_words || all_items.is_empty() {
                single_suffix_item(ctx, tracker).parse_next(input)
            } else {
                io_redirect(ctx, tracker)
                    .map(ast::CommandPrefixOrSuffixItem::IoRedirect)
                    .parse_next(input)
            };
            match item {
                Ok(item) => {
                    all_items.push(item);
                    spaces().parse_next(input)?;
                }
                Err(winnow::error::ErrMode::Cut(e)) => return Err(winnow::error::ErrMode::Cut(e)),
                Err(_) => break,
            }
        }

        if all_items.is_empty() {
            return fail.parse_next(input);
        }

        Ok(ast::CommandSuffix(all_items))
    }
}

/// Parse a simple command (command name + optional arguments)
/// Now supports: prefix (assignments/redirects) + optional command + optional suffix
/// Corresponds to: peg.rs `simple_command()`
pub(super) fn simple_command<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::SimpleCommand, ContextError> + 'a {
    trace("simple_command", move |input: &mut StrStream<'a>| {
        // Try to parse optional prefix (assignments and/or redirects)
        let prefix = winnow::combinator::opt(cmd_prefix(ctx, tracker)).parse_next(input)?;

        // Try to parse optional command name (must not be reserved word).
        // N.B. Must use opt() rather than .ok() so the input position is
        // restored on failure — .ok() discards errors without backtracking.
        let word_or_name =
            winnow::combinator::opt(non_reserved_word(ctx, tracker)).parse_next(input)?;

        // Try to parse optional suffix (args and/or redirects)
        let suffix = winnow::combinator::opt(cmd_suffix(ctx, tracker)).parse_next(input)?;

        // Must have at least one of: prefix, word, or suffix
        if prefix.is_none() && word_or_name.is_none() && suffix.is_none() {
            return fail.parse_next(input);
        }

        Ok(ast::SimpleCommand {
            prefix,
            word_or_name,
            suffix,
        })
    })
}

/// Parse a simple command or a function definition, whichever the command looks like
/// first: a name followed by `()` is tried as a function definition first.
fn simple_command_or_function<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::Command, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Function names may contain hyphens, dots, and other non-metacharacters
        // (see bash manual, §Shell Functions).
        let peeked: ModalResult<_> = winnow::combinator::peek((
            take_while(1.., |c: char| {
                c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '+' | '@' | '/')
            }),
            super::helpers::opt_blanks(),
            "()",
        ))
        .parse_next(input);
        let looks_like_function = peeked.is_ok();

        let simple = || simple_command(ctx, tracker).map(ast::Command::Simple);
        let function =
            || super::compound::function_definition(ctx, tracker).map(ast::Command::Function);
        if looks_like_function {
            winnow::combinator::alt((function(), simple())).parse_next(input)
        } else {
            winnow::combinator::alt((simple(), function())).parse_next(input)
        }
    }
}

/// Parse a compound command with its redirections.
fn compound_with_redirects<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::Command, ContextError> + 'a {
    (
        compound_command(ctx, tracker),
        optional_redirects(ctx, tracker),
    )
        .map(|(c, r)| ast::Command::Compound(c, r))
}

/// Parse a command (simple or compound).
/// Uses keyword dispatch for performance - dispatches based on first word/char
/// to avoid trying all compound command parsers for simple commands.
pub(super) fn command<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::Command, ContextError> + 'a {
    trace("command", move |input: &mut StrStream<'a>| {
        spaces().parse_next(input)?; // Consume optional leading spaces

        // Fast path: dispatch based on first character
        let Ok(first_char) = peek_char().parse_next(input) else {
            return fail.parse_next(input);
        };

        match first_char {
            // Brace group, subshell or arithmetic command
            '{' | '(' => compound_with_redirects(ctx, tracker).parse_next(input),
            // Extended test [[ ... ]] in bash mode; a single [ is the test command
            '[' if !ctx.options.posix_mode
                && !ctx.options.sh_mode
                && input.as_ref().starts_with("[[") =>
            {
                compound_with_redirects(ctx, tracker).parse_next(input)
            }
            // Alphabetic: could be keyword or simple command
            c if c.is_alphabetic() || c == '_' => {
                let Ok(word) = peek_first_word().parse_next(input) else {
                    return simple_command_or_function(ctx, tracker).parse_next(input);
                };
                // A reserved word assigned to (`case=1`, `in+=x`, `if[0]=y`) is a variable.
                let assigned: ModalResult<_> = winnow::combinator::peek((
                    take_while(1.., |c: char| c.is_alphanumeric() || c == '_'),
                    winnow::combinator::alt(("=", "+=", "[")),
                ))
                .parse_next(input);
                if assigned.is_ok() {
                    return simple_command(ctx, tracker)
                        .map(ast::Command::Simple)
                        .parse_next(input);
                }
                match word {
                    "if" | "while" | "until" | "for" | "case" | "coproc" => {
                        compound_with_redirects(ctx, tracker).parse_next(input)
                    }
                    "function" => super::compound::function_definition(ctx, tracker)
                        .map(ast::Command::Function)
                        .parse_next(input),
                    // Reserved words that end compound commands, and `in`, which ends a
                    // `for` or `case` word list, fail cleanly.
                    "then" | "else" | "elif" | "fi" | "do" | "done" | "esac" | "in" => {
                        fail.parse_next(input)
                    }
                    _ => simple_command_or_function(ctx, tracker).parse_next(input),
                }
            }
            _ => simple_command_or_function(ctx, tracker).parse_next(input),
        }
    })
}
