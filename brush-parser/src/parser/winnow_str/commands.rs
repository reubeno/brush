use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::token::take_while;

use crate::ast;

use super::arithmetic::for_or_arithmetic_for;
use super::arithmetic::paren_compound;
use super::compound::{
    brace_group, case_clause, if_clause, process_substitution, until_clause, while_clause,
};
use super::extended_test::extended_test_command;
use super::helpers::{
    array_spaces, parse_balanced_delimiters, peek_char, peek_first_word, spaces, spaces1,
};
use super::position::PositionTracker;
use super::redirections::{here_documents, io_number, io_redirect, optional_redirects};
use super::types::{PError, ParseContext, StrStream};
use super::words::{non_reserved_word, word_as_ast, word_part};

// ============================================================================
// Tier 3: Commands
// ============================================================================

/// Parse an array element value (handles quotes properly, stops at ')' or whitespace)
fn array_element_value<'a>(
    ctx: &'a ParseContext<'a>,
) -> impl Parser<StrStream<'a>, String, PError> + 'a {
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
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        }

        Ok(value)
    }
}

/// Parse an array element: either "value" or "[index]=value"
fn array_element<'a>(
    ctx: &'a ParseContext<'a>,
) -> impl Parser<StrStream<'a>, (Option<ast::Word>, ast::Word), PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Skip whitespace before element (newlines are whitespace inside arrays)
        array_spaces().parse_next(input)?;

        // Try to parse indexed element: [index]=value
        let checkpoint = input.checkpoint();
        let has_bracket = winnow::combinator::peek::<_, _, PError, _>('[')
            .parse_next(input)
            .is_ok();

        if has_bracket {
            // Parse index using parse_balanced_delimiters to handle nested brackets
            let index_str_with_brackets =
                parse_balanced_delimiters("[", Some('['), ']', 1).parse_next(input)?;
            // Strip the outer brackets to match PEG parser behavior
            let index_str = &index_str_with_brackets[1..index_str_with_brackets.len() - 1];

            let has_close = true; // parse_balanced_delimiters already consumed the ']'
            let has_equals = winnow::combinator::opt::<_, _, PError, _>('=')
                .parse_next(input)?
                .is_some();

            if has_close && has_equals {
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

/// Parse an assignment word (VAR=value or VAR+=value or VAR[idx]=value or VAR=(array elements))
/// Returns (Assignment, original word as `ast::Word`)
fn assignment_word<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, (ast::Assignment, ast::Word), PError> + 'a {
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
        let array_index = if winnow::combinator::peek::<_, _, PError, _>('[')
            .parse_next(input)
            .is_ok()
        {
            // Parse the index using parse_balanced_delimiters to handle nested brackets
            let index_with_brackets =
                parse_balanced_delimiters("[", Some('['), ']', 1).parse_next(input)?;
            // Strip the outer brackets to match PEG parser behavior
            let index = &index_with_brackets[1..index_with_brackets.len() - 1];
            Some(index.to_string())
        } else {
            None
        };

        // Check for optional '+' (append assignment)
        let append = winnow::combinator::opt('+').parse_next(input)?.is_some();

        // Must have '='
        '='.parse_next(input)?;

        // Check if it's an array assignment
        let checkpoint = input.checkpoint();
        let has_paren = winnow::combinator::opt::<_, _, PError, _>('(')
            .parse_next(input)?
            .is_some();

        if has_paren {
            // Parse array elements
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
                if winnow::combinator::opt::<_, _, PError, _>(')')
                    .parse_next(input)?
                    .is_some()
                {
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
) -> impl Parser<StrStream<'a>, ast::CommandPrefix, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        winnow::combinator::repeat::<_, _, Vec<_>, _, _>(
            1..,
            winnow::combinator::terminated(
                winnow::combinator::alt((
                    io_redirect(ctx, tracker)
                        .map(|r| ast::CommandPrefixOrSuffixItem::IoRedirect(r.redirect)),
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

/// Check if we're at a here-doc marker (<<) but NOT a here-string (<<<)
fn at_here_doc_marker<'a>() -> impl Parser<StrStream<'a>, (), PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Skip optional fd number
        winnow::combinator::opt(io_number()).parse_next(input)?;

        // Check for << but not <<<
        let checkpoint = input.checkpoint();
        if winnow::combinator::opt::<_, _, PError, _>("<<")
            .parse_next(input)?
            .is_some()
        {
            // Make sure it's not <<<
            if winnow::combinator::peek::<_, _, PError, _>('<')
                .parse_next(input)
                .is_ok()
            {
                // It's <<<, not <<
                input.reset(&checkpoint);
                return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
            }
            input.reset(&checkpoint);
            Ok(())
        } else {
            input.reset(&checkpoint);
            Err(winnow::error::ErrMode::Backtrack(ContextError::default()))
        }
    }
}

/// Parse multiple here-docs when we know we're at a here-doc marker.
/// Returns a Vec of `IoRedirect` items.
fn parse_here_docs<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, Vec<ast::CommandPrefixOrSuffixItem>, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let (docs, remaining) = here_documents(tracker).parse_next(input)?;

        // Store trailing content in context for later processing by pipe_sequence
        if let Some(trailing) = remaining {
            *ctx.pending_heredoc_trailing.borrow_mut() = Some(trailing);
        }

        let items: Vec<ast::CommandPrefixOrSuffixItem> = docs
            .into_iter()
            .map(|(fd, doc)| {
                ast::CommandPrefixOrSuffixItem::IoRedirect(ast::IoRedirect::HereDocument(fd, doc))
            })
            .collect();

        Ok(items)
    }
}

/// Parse a single suffix item (word, redirect, process substitution, or assignment).
fn single_suffix_item<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::CommandPrefixOrSuffixItem, PError> + 'a {
    winnow::combinator::alt((
        io_redirect(ctx, tracker).map(|r| ast::CommandPrefixOrSuffixItem::IoRedirect(r.redirect)),
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
/// Corresponds to: winnow.rs `cmd_suffix()`.
pub(super) fn cmd_suffix<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::CommandSuffix, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Require at least one space before suffix items
        spaces1().parse_next(input)?;

        let mut all_items: Vec<ast::CommandPrefixOrSuffixItem> = Vec::new();

        loop {
            // Fast path: peek at first char to decide what to try
            let Ok(ch) = peek_char().parse_next(input) else {
                break;
            };

            // Only check for here-docs when we see '<' or a digit (fd number)
            if ch == '<' || ch.is_ascii_digit() {
                // Check if this is a here-doc (but not here-string)
                if winnow::combinator::peek::<_, _, PError, _>(at_here_doc_marker())
                    .parse_next(input)
                    .is_ok()
                {
                    let items = parse_here_docs(ctx, tracker).parse_next(input)?;
                    all_items.extend(items);
                    // Heredoc resolution consumed the command-line newline and
                    // all heredoc content lines.  Any trailing content on the
                    // same line (e.g., "| grep") is in pending_heredoc_trailing
                    // and handled by pipe_sequence.  We must stop parsing
                    // suffix items here — the next line is a new command.
                    break;
                }
            }

            // Try to parse a single suffix item
            match single_suffix_item(ctx, tracker).parse_next(input) {
                Ok(item) => {
                    all_items.push(item);
                    spaces().parse_next(input)?;
                }
                Err(_) => break,
            }
        }

        if all_items.is_empty() {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
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
) -> impl Parser<StrStream<'a>, ast::SimpleCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
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
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        }

        Ok(ast::SimpleCommand {
            prefix,
            word_or_name,
            suffix,
        })
    }
}

/// Parse a command (simple or compound).
///
/// Corresponds to: winnow.rs `command()`.
/// Uses keyword dispatch for performance - dispatches based on first word/char
/// to avoid trying all compound command parsers for simple commands.
#[allow(clippy::too_many_lines)]
pub(super) fn command<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::Command, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        spaces().parse_next(input)?; // Consume optional leading spaces

        // Fast path: dispatch based on first character
        let Ok(first_char) = peek_char().parse_next(input) else {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        };

        match first_char {
            // Brace group: { ... }
            '{' => (brace_group(ctx, tracker), optional_redirects(ctx, tracker))
                .map(|(c, r)| ast::Command::Compound(ast::CompoundCommand::BraceGroup(c), r))
                .parse_next(input),

            // Parenthesized: subshell ( ... ) or arithmetic (( ... ))
            '(' => (
                paren_compound(ctx, tracker),
                optional_redirects(ctx, tracker),
            )
                .map(|(c, r)| ast::Command::Compound(c, r))
                .parse_next(input),

            // Extended test: [[ ... ]] (bash mode only)
            '[' if !ctx.options.posix_mode && !ctx.options.sh_mode => {
                // Check if it's [[
                if winnow::combinator::peek::<_, _, PError, _>("[[")
                    .parse_next(input)
                    .is_ok()
                {
                    (
                        extended_test_command(ctx, tracker),
                        optional_redirects(ctx, tracker),
                    )
                        .map(|(cmd, r)| ast::Command::ExtendedTest(cmd, r))
                        .parse_next(input)
                } else {
                    // Single [ is the test command (simple command)
                    simple_command(ctx, tracker)
                        .map(ast::Command::Simple)
                        .parse_next(input)
                }
            }

            // Alphabetic: could be keyword or simple command
            c if c.is_alphabetic() || c == '_' => {
                // Peek the first word to dispatch on keywords
                if let Ok(word) = peek_first_word().parse_next(input) {
                    match word {
                        "if" => (if_clause(ctx, tracker), optional_redirects(ctx, tracker))
                            .map(|(c, r)| {
                                ast::Command::Compound(ast::CompoundCommand::IfClause(c), r)
                            })
                            .parse_next(input),
                        "while" => (while_clause(ctx, tracker), optional_redirects(ctx, tracker))
                            .map(|(c, r)| {
                                ast::Command::Compound(ast::CompoundCommand::WhileClause(c), r)
                            })
                            .parse_next(input),
                        "until" => (until_clause(ctx, tracker), optional_redirects(ctx, tracker))
                            .map(|(c, r)| {
                                ast::Command::Compound(ast::CompoundCommand::UntilClause(c), r)
                            })
                            .parse_next(input),
                        "for" => (
                            for_or_arithmetic_for(ctx, tracker),
                            optional_redirects(ctx, tracker),
                        )
                            .map(|(c, r)| ast::Command::Compound(c, r))
                            .parse_next(input),
                        "case" => (case_clause(ctx, tracker), optional_redirects(ctx, tracker))
                            .map(|(c, r)| {
                                ast::Command::Compound(ast::CompoundCommand::CaseClause(c), r)
                            })
                            .parse_next(input),
                        "function" => super::compound::function_definition(ctx, tracker)
                            .map(ast::Command::Function)
                            .parse_next(input),
                        // Reserved words that terminate compound commands - fail cleanly
                        "then" | "else" | "elif" | "fi" | "do" | "done" | "esac" | "in" => {
                            Err(winnow::error::ErrMode::Backtrack(ContextError::default()))
                        }
                        // Not a keyword - check if it looks like a function definition (name followed by ())
                        _ => {
                            // Peek for function definition pattern: name + optional_spaces + "()"
                            // Function names may contain hyphens, dots, and other
                            // non-metacharacters (see bash manual, §Shell Functions).
                            let is_func_def = winnow::combinator::peek::<_, _, PError, _>((
                                take_while(1.., |c: char| {
                                    c.is_alphanumeric()
                                        || matches!(c, '_' | '-' | '.' | ':' | '+' | '@')
                                }),
                                take_while(0.., |c| c == ' ' || c == '\t'),
                                "()",
                            ))
                            .parse_next(input)
                            .is_ok();

                            if is_func_def {
                                winnow::combinator::alt((
                                    super::compound::function_definition(ctx, tracker)
                                        .map(ast::Command::Function),
                                    simple_command(ctx, tracker).map(ast::Command::Simple),
                                ))
                                .parse_next(input)
                            } else {
                                // Regular command - try simple command first
                                winnow::combinator::alt((
                                    simple_command(ctx, tracker).map(ast::Command::Simple),
                                    super::compound::function_definition(ctx, tracker)
                                        .map(ast::Command::Function),
                                ))
                                .parse_next(input)
                            }
                        }
                    }
                } else {
                    // Can't peek word, try simple command
                    simple_command(ctx, tracker)
                        .map(ast::Command::Simple)
                        .parse_next(input)
                }
            }

            // Other characters: likely simple command (variable assignment, redirect, etc.)
            _ => winnow::combinator::alt((
                simple_command(ctx, tracker).map(ast::Command::Simple),
                super::compound::function_definition(ctx, tracker).map(ast::Command::Function),
            ))
            .parse_next(input),
        }
    }
}
