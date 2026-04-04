use winnow::combinator::{dispatch, fail, repeat};
use winnow::error::ContextError;
use winnow::prelude::*;

use crate::ast;

use super::and_or::and_or;
use super::arithmetic::for_or_arithmetic_for;
use super::helpers::{
    fname, is_reserved_word, keyword, linebreak, name, newline, peek_char, peek_first_word,
    separator, sequential_sep, spaces, spaces1,
};
use super::position::PositionTracker;
use super::redirections::redirect_list;
use super::types::{ParseContext, StrStream};
use super::words::word_as_ast;

// ============================================================================
// Tier 10: Subshells and Command Groups
// ============================================================================

/// Parse a compound list (used inside subshells, brace groups, etc.)
///
/// Similar to `complete_command` but with optional leading linebreaks and more flexible separators
/// Corresponds to: winnow.rs `compound_list()`
pub(super) fn compound_list<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::CompoundList, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Optional leading linebreaks
        linebreak().parse_next(input)?;

        // Parse first and_or (required)
        let mut current_ao = and_or(ctx, tracker).parse_next(input)?;
        let mut items: Vec<ast::CompoundListItem> = vec![];

        // Try to parse (separator + and_or) pairs
        // Note: Manual loop is faster than repeat() combinator here due to early break optimization
        loop {
            // Try to get separator after current and_or (handles both ; & and newlines)
            spaces().parse_next(input)?;

            let sep_opt = if let Ok(sep_opt) = separator().parse_next(input) {
                spaces().parse_next(input)?;
                sep_opt
            } else {
                // No separator - add current and_or with default separator and we're done
                items.push(ast::CompoundListItem(
                    current_ao,
                    ast::SeparatorOperator::Sequence,
                ));
                break;
            };

            // Convert Option<SeparatorOperator> to SeparatorOperator (None means newline, treat as
            // Sequence)
            let sep = sep_opt.unwrap_or(ast::SeparatorOperator::Sequence);

            // Push current and_or with its separator
            items.push(ast::CompoundListItem(current_ao, sep));

            // We have a separator, check if there's another and_or after it
            if let Ok(next_ao) = and_or(ctx, tracker).parse_next(input) {
                // Move to next
                current_ao = next_ao;
            } else {
                // Trailing separator
                break;
            }
        }

        Ok(ast::CompoundList(items))
    }
}

/// Parse a subshell: ( commands )
/// Corresponds to: winnow.rs `subshell()`
pub(super) fn subshell<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::SubshellCommand, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let (list, range) = winnow::combinator::delimited(
            ('(', spaces(), linebreak()),
            compound_list(ctx, tracker),
            (linebreak(), spaces(), ')'),
        )
        .with_span()
        .parse_next(input)?;

        Ok(ast::SubshellCommand {
            list,
            loc: tracker.range_to_span(range),
        })
    }
}

/// Parse a brace group: { commands; }
/// Corresponds to: winnow.rs `brace_group()`
pub(super) fn brace_group<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::BraceGroupCommand, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let (list, range) = winnow::combinator::delimited(
            // IMPORTANT: Require at least one space OR newline after '{'
            // This distinguishes brace groups from brace expansion:
            // - Brace group: { echo hello; } (requires space after {)
            // - Brace expansion: {1..10} (no space, part of word)
            (
                '{',
                winnow::combinator::alt((
                    spaces1(),        // At least one space/tab
                    newline().void(), // Or a newline
                )),
            ),
            compound_list(ctx, tracker),
            // Before '}': optional linebreak and spaces
            // Note: A separator (;/&) or newline is required before }, but that's
            // handled by compound_list. We just allow optional additional whitespace.
            (linebreak(), spaces(), '}'),
        )
        .with_span()
        .parse_next(input)?;

        Ok(ast::BraceGroupCommand {
            list,
            loc: tracker.range_to_span(range),
        })
    }
}

/// Parse process substitution: <(command) or >(command)
/// Corresponds to: peg.rs `process_substitution()`
pub(super) fn process_substitution<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, (ast::ProcessSubstitutionKind, ast::SubshellCommand), ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        // Parse < or > to determine the kind
        let kind = winnow::combinator::alt((
            "<".value(ast::ProcessSubstitutionKind::Read),
            ">".value(ast::ProcessSubstitutionKind::Write),
        ))
        .parse_next(input)?;

        // Then parse the subshell-like content: ( compound_list )
        let list = winnow::combinator::delimited(
            ('(', spaces(), linebreak()),
            compound_list(ctx, tracker),
            (linebreak(), spaces(), ')'),
        )
        .parse_next(input)?;

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok((kind, ast::SubshellCommand { list, loc }))
    }
}

// ============================================================================
// Tier 11: Compound Commands (if, while, until, for, case)
// ============================================================================

/// Parse a do group: do ... done
/// Corresponds to: winnow.rs `do_group()`
pub(super) fn do_group<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::DoGroupCommand, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let (list, range) = winnow::combinator::delimited(
            keyword("do"),
            compound_list(ctx, tracker), // compound_list handles its own leading linebreak
            keyword("done"),
        )
        .with_span()
        .parse_next(input)?;

        Ok(ast::DoGroupCommand {
            list,
            loc: tracker.range_to_span(range),
        })
    }
}

/// Parse an elif clause
fn elif_clause<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::ElseClause, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        keyword("elif").parse_next(input)?;
        let condition = compound_list(ctx, tracker).parse_next(input)?;
        keyword("then").parse_next(input)?;
        let body = compound_list(ctx, tracker).parse_next(input)?;
        Ok(ast::ElseClause {
            condition: Some(condition),
            body,
        })
    }
}

/// Parse an else clause (final, no condition)
fn else_clause<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::ElseClause, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        keyword("else").parse_next(input)?;
        let body = compound_list(ctx, tracker).parse_next(input)?;
        Ok(ast::ElseClause {
            condition: None,
            body,
        })
    }
}

/// Parse an if clause: if ... then ... [elif ... then ...]* [else ...] fi
/// Corresponds to: winnow.rs `if_clause()`
pub(super) fn if_clause<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::IfClauseCommand, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        keyword("if").parse_next(input)?;
        let condition = compound_list(ctx, tracker).parse_next(input)?;
        keyword("then").parse_next(input)?;
        let then_body = compound_list(ctx, tracker).parse_next(input)?;

        // Parse elif clauses (zero or more)
        let mut elses: Vec<ast::ElseClause> =
            repeat(0.., elif_clause(ctx, tracker)).parse_next(input)?;

        // Parse optional else clause
        if let Ok(else_part) = else_clause(ctx, tracker).parse_next(input) {
            elses.push(else_part);
        }

        keyword("fi").parse_next(input)?;

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::IfClauseCommand {
            condition,
            then: then_body,
            elses: if elses.is_empty() { None } else { Some(elses) },
            loc,
        })
    }
}

/// Parse a while clause: while ... do ... done
/// Corresponds to: winnow.rs `while_clause()`
pub(super) fn while_clause<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::WhileOrUntilClauseCommand, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        keyword("while").parse_next(input)?;
        let condition = compound_list(ctx, tracker).parse_next(input)?;
        let body = do_group(ctx, tracker).parse_next(input)?;

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::WhileOrUntilClauseCommand(condition, body, loc))
    }
}

/// Parse an until clause: until ... do ... done
/// Corresponds to: winnow.rs `until_clause()`
pub(super) fn until_clause<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::WhileOrUntilClauseCommand, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        keyword("until").parse_next(input)?;
        let condition = compound_list(ctx, tracker).parse_next(input)?;
        let body = do_group(ctx, tracker).parse_next(input)?;

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::WhileOrUntilClauseCommand(condition, body, loc))
    }
}

// ============================================================================
// Tier 12: For Loops
// ============================================================================

/// Parse a for clause: for var in list; do ... done
/// Corresponds to: winnow.rs `for_clause()`
pub(super) fn for_clause<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::ForClauseCommand, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        keyword("for").parse_next(input)?;
        let var_name = name().parse_next(input)?;

        linebreak().parse_next(input)?;

        if winnow::combinator::opt(keyword("in")).parse_next(input)?.is_some() {
            let values = winnow::combinator::opt(winnow::combinator::preceded(
                spaces(),
                winnow::combinator::separated(1.., word_as_ast(ctx, tracker), spaces1()),
            ))
            .parse_next(input)?;

            sequential_sep().parse_next(input)?;
            let body = do_group(ctx, tracker).parse_next(input)?;

            let end_offset = tracker.offset_from_locating(input);
            let loc = tracker.range_to_span(start_offset..end_offset);

            Ok(ast::ForClauseCommand {
                variable_name: var_name,
                values,
                body,
                loc,
            })
        } else {
            winnow::combinator::opt(sequential_sep()).parse_next(input)?;
            let body = do_group(ctx, tracker).parse_next(input)?;

            let end_offset = tracker.offset_from_locating(input);
            let loc = tracker.range_to_span(start_offset..end_offset);

            Ok(ast::ForClauseCommand {
                variable_name: var_name,
                values: None,
                body,
                loc,
            })
        }
    }
}

// ============================================================================
// Tier 13: Case Statements
// ============================================================================

/// Parse case item terminator (;;, ;&, or ;;&)
fn case_item_terminator<'a>() -> impl ModalParser<StrStream<'a>, ast::CaseItemPostAction, ContextError> {
    winnow::combinator::preceded(
        spaces(),
        dispatch! {super::helpers::peek_op3();
            ";;&" => ";;&".value(ast::CaseItemPostAction::ContinueEvaluatingCases),
            ";&" => ";&".value(ast::CaseItemPostAction::UnconditionallyExecuteNextCaseItem),
            ";;" => ";;".value(ast::CaseItemPostAction::ExitCase),
            _ => fail,
        },
    )
}

/// Parse a case item: pattern) commands ;;
fn case_item<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::CaseItem, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        spaces().parse_next(input)?;
        let start_offset = tracker.offset_from_locating(input);

        // Optional leading (
        let _ = winnow::combinator::opt('(').parse_next(input)?;
        spaces().parse_next(input)?;

        // Parse patterns: word separated by |
        let patterns: Vec<ast::Word> = winnow::combinator::separated(
            1..,
            word_as_ast(ctx, tracker),
            winnow::combinator::preceded(spaces(), winnow::combinator::terminated('|', spaces())),
        )
        .parse_next(input)?;

        spaces().parse_next(input)?;
        ')'.parse_next(input)?;

        linebreak().parse_next(input)?;

        // Parse body (optional)
        let cmd = winnow::combinator::opt(compound_list(ctx, tracker)).parse_next(input)?;

        // Parse case item terminator (optional - default to ExitCase)
        let post_action = winnow::combinator::opt(case_item_terminator())
            .parse_next(input)?
            .unwrap_or(ast::CaseItemPostAction::ExitCase);

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        linebreak().parse_next(input)?;

        Ok(ast::CaseItem {
            patterns,
            cmd,
            post_action,
            loc: Some(loc),
        })
    }
}

/// Parse case list (multiple case items until "esac")
fn case_list<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, Vec<ast::CaseItem>, ContextError> + 'a {
    winnow::combinator::repeat(1.., case_item(ctx, tracker))
}

/// Parse a case clause: case word in patterns) commands ;; esac
/// Corresponds to: winnow.rs `case_clause()`
pub(super) fn case_clause<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::CaseClauseCommand, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        keyword("case").parse_next(input)?;
        spaces().parse_next(input)?;
        let target = word_as_ast(ctx, tracker).parse_next(input)?;

        linebreak().parse_next(input)?;
        keyword("in").parse_next(input)?;
        linebreak().parse_next(input)?;

        // Use opt() for optional case list
        let items = winnow::combinator::opt(case_list(ctx, tracker)).parse_next(input)?;

        spaces().parse_next(input)?;
        keyword("esac").parse_next(input)?;

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::CaseClauseCommand {
            value: target,
            cases: items.unwrap_or_default(),
            loc,
        })
    }
}

/// Parse a coproc clause: coproc [NAME] command
/// Corresponds to: bash coproc syntax
pub(super) fn coproc_clause<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::CoprocessCommand, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        keyword("coproc").parse_next(input)?;
        spaces().parse_next(input)?;

        // Try to parse an optional name
        // The name must be followed by a compound command ({ or ()
        // Otherwise, the "name" is actually the start of a simple command body
        let checkpoint = input.checkpoint();
        let name = if let Ok(word) = fname().parse_next(input) {
            // Check if this looks like a name followed by a compound command
            spaces().parse_next(input)?;
            if let Ok(ch) = peek_char().parse_next(input) {
                if ch == '{' || ch == '(' {
                    Some(ast::Word::new(&word))
                } else {
                    // Not a compound command after name - backtrack
                    // The word is actually the command name, restore position
                    input.reset(&checkpoint);
                    None
                }
            } else {
                // End of input after word - backtrack
                input.reset(&checkpoint);
                None
            }
        } else {
            None
        };

        // Parse the body as a command (simple or compound)
        let body = Box::new(super::commands::command(ctx, tracker).parse_next(input)?);

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        Ok(ast::CoprocessCommand { name, body, loc })
    }
}

// ============================================================================
// Tier 14: Function Definitions
// ============================================================================

/// Parse a compound command - tries all compound command types
/// Corresponds to: winnow.rs `compound_command()`
pub(super) fn compound_command<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::CompoundCommand, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        winnow::combinator::preceded(
            spaces(),
            dispatch! {peek_char();
                '{' => brace_group(ctx, tracker).map(ast::CompoundCommand::BraceGroup),
                '(' => super::arithmetic::paren_compound(ctx, tracker),  // Handles both (( )) arithmetic and ( ) subshell
                'c' => case_or_coproc(ctx, tracker),  // Handles both case and coproc
                'f' => for_or_arithmetic_for(ctx, tracker),  // Handles both for (( )) and for name in
                'i' => if_clause(ctx, tracker).map(ast::CompoundCommand::IfClause),
                'w' => while_clause(ctx, tracker).map(ast::CompoundCommand::WhileClause),
                'u' => until_clause(ctx, tracker).map(ast::CompoundCommand::UntilClause),
                _ => fail,
            },
        )
        .parse_next(input)
    }
}

/// Parse either case or coproc clause (both start with 'c')
fn case_or_coproc<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::CompoundCommand, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Peek at the word to determine which one
        if let Ok(word) = peek_first_word().parse_next(input) {
            match word {
                "case" => case_clause(ctx, tracker)
                    .map(ast::CompoundCommand::CaseClause)
                    .parse_next(input),
                "coproc" => coproc_clause(ctx, tracker)
                    .map(ast::CompoundCommand::Coprocess)
                    .parse_next(input),
                _ => fail.parse_next(input),
            }
        } else {
            fail.parse_next(input)
        }
    }
}

/// Parse function body (compound command with optional redirects)
/// Corresponds to: winnow.rs `function_body()`
fn function_body<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::FunctionBody, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let cmd = compound_command(ctx, tracker).parse_next(input)?;
        let redirects = winnow::combinator::opt(winnow::combinator::preceded(
            spaces(),
            redirect_list(ctx, tracker),
        ))
        .parse_next(input)?;

        Ok(ast::FunctionBody(cmd, redirects))
    }
}

/// Parse function definition
/// Corresponds to: winnow.rs `function_definition()`
pub(super) fn function_definition<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::FunctionDefinition, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Try "function name () body" or "function name body" format
        let has_function_keyword = winnow::combinator::opt(keyword("function")).parse_next(input)?.is_some();

        // Track location of the function name
        let fname_start = tracker.offset_from_locating(input);
        let func_name = fname().parse_next(input)?;
        let fname_end = tracker.offset_from_locating(input);

        // Function names cannot be reserved words (unless preceded by `function` keyword)
        if !has_function_keyword && is_reserved_word(&func_name) {
            return fail.parse_next(input);
        }

        // Parse optional ()
        spaces().parse_next(input)?;
        let has_parens = if winnow::combinator::opt('(')
            .parse_next(input)?
            .is_some()
        {
            spaces().parse_next(input)?;
            ')'.parse_next(input)?;
            true
        } else {
            false
        };

        // Must have either "function" keyword or parens
        if !has_function_keyword && !has_parens {
            return fail.parse_next(input);
        }

        linebreak().parse_next(input)?;

        let body = function_body(ctx, tracker).parse_next(input)?;

        // Create the fname Word with location
        let fname_loc = tracker.range_to_span(fname_start..fname_end);
        let fname_word = ast::Word {
            value: func_name,
            loc: Some(fname_loc),
        };

        Ok(ast::FunctionDefinition {
            fname: fname_word,
            body,
        })
    }
}

#[cfg(test)]
mod coproc_tests {
    use super::*;
    use crate::parser::{ParserOptions, SourceInfo};

    fn parse_with_winnow(input: &str) -> Result<ast::Program, crate::error::ParseError> {
        super::super::program::parse_program(
            input,
            &ParserOptions::default(),
            &SourceInfo::default(),
        )
    }

    #[test]
    fn test_coproc_simple() {
        let result = parse_with_winnow("coproc echo hello");
        assert!(result.is_ok(), "Simple coproc should parse: {result:?}");
    }

    #[test]
    fn test_coproc_brace_group() {
        let result = parse_with_winnow("coproc { echo hello; }");
        assert!(
            result.is_ok(),
            "Coproc with brace group should parse: {result:?}"
        );
    }

    #[test]
    fn test_coproc_named_brace_group() {
        let result = parse_with_winnow("coproc NAME { echo hello; }");
        assert!(
            result.is_ok(),
            "Coproc with named brace group should parse: {result:?}"
        );
    }

    #[test]
    fn test_coproc_subshell() {
        let result = parse_with_winnow("coproc (echo hello)");
        assert!(
            result.is_ok(),
            "Coproc with subshell should parse: {result:?}"
        );
    }
}
