use winnow::combinator::repeat;
use winnow::prelude::*;
use winnow::stream::LocatingSlice;

use crate::ast;
use crate::parser::{ParserOptions, SourceInfo};

use super::and_or::and_or;
use super::helpers::{comment, linebreak, newline_list, separator_op, spaces};
use super::position::PositionTracker;
use super::types::{PError, ParseContext, StrStream};

// ============================================================================
// Tier 6: Complete Commands and Programs
// ============================================================================

/// Parse a complete command (and/or lists with separators)
/// Corresponds to: winnow.rs `complete_command()`
pub(super) fn complete_command<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::CompleteCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        // Parse first and_or (required)
        let first_ao = and_or(ctx, tracker).parse_next(input)?;

        // Try to parse (separator + spaces + and_or) pairs
        let mut items: Vec<ast::CompoundListItem> = vec![];

        // Try to get separator after first and_or
        spaces().parse_next(input)?; // Consume spaces
        if let Ok(sep) = separator_op().parse_next(input) {
            spaces().parse_next(input)?; // Consume spaces after separator

            // First item has a separator
            items.push(ast::CompoundListItem(first_ao, sep));

            // Parse remaining (and_or, separator) pairs
            loop {
                // Try to parse next and_or
                let Ok(ao) = and_or(ctx, tracker).parse_next(input) else {
                    break;
                };

                // Try to get separator
                spaces().parse_next(input)?;
                if let Ok(sep) = separator_op().parse_next(input) {
                    spaces().parse_next(input)?;
                    items.push(ast::CompoundListItem(ao, sep));
                } else {
                    // No separator - this is the final and_or
                    items.push(ast::CompoundListItem(ao, ast::SeparatorOperator::Sequence));
                    break;
                }
            }
        } else {
            // No separator - just one and_or
            items.push(ast::CompoundListItem(
                first_ao,
                ast::SeparatorOperator::Sequence,
            ));
        }

        Ok(ast::CompoundList(items))
    }
}

/// Parse a newline-separated complete command continuation
/// Corresponds to: winnow.rs `complete_command_continuation()`
fn complete_command_continuation<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::CompleteCommand, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        winnow::combinator::preceded(newline_list(), complete_command(ctx, tracker))
            .parse_next(input)
    }
}

/// Parse multiple complete commands separated by newlines
/// Corresponds to: winnow.rs `complete_commands()`
pub(super) fn complete_commands<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, Vec<ast::CompleteCommand>, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        (
            complete_command(ctx, tracker),
            repeat::<_, _, Vec<_>, _, _>(0.., complete_command_continuation(ctx, tracker)),
        )
            .map(
                |(first, rest): (ast::CompleteCommand, Vec<ast::CompleteCommand>)| {
                    let mut commands = Vec::with_capacity(1 + rest.len());
                    commands.push(first);
                    commands.extend(rest);
                    commands
                },
            )
            .parse_next(input)
    }
}

/// Parse a complete program
/// Corresponds to: winnow.rs `program()`
pub(super) fn program<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::Program, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        linebreak().parse_next(input)?;
        let complete_commands = winnow::combinator::opt(complete_commands(ctx, tracker))
            .parse_next(input)?
            .unwrap_or_default();
        linebreak().parse_next(input)?;
        // Consume any trailing whitespace/comment that isn't followed by a newline
        // (e.g., a comment at the end of a file without a trailing newline).
        let _: &str =
            winnow::token::take_while(0.., |c: char| c == ' ' || c == '\t').parse_next(input)?;
        winnow::combinator::opt(comment()).parse_next(input)?;
        winnow::combinator::eof.parse_next(input)?;
        Ok(ast::Program { complete_commands })
    }
}

/// Parse a shell program from a string with full source location tracking
///
/// This is the main entry point for parsing shell scripts using the `winnow_str` parser.
/// It creates a `PositionTracker` for efficient line/column lookup and parses the entire program.
///
/// # Arguments
/// * `input` - The shell script source code to parse
/// * `options` - Parser options controlling extended globbing, POSIX mode, etc.
/// * `source_info` - Source file information for error reporting
///
/// # Example
/// ```ignore
/// use brush_parser::parser::winnow_str::parse_program;
/// use brush_parser::parser::{ParserOptions, SourceInfo};
///
/// let result = parse_program("echo hello", &ParserOptions::default(), &SourceInfo::default());
/// ```
pub fn parse_program(
    input: &str,
    options: &ParserOptions,
    source_info: &SourceInfo,
) -> Result<ast::Program, PError> {
    let pending_heredoc_trailing = std::cell::RefCell::new(None);
    let ctx = ParseContext {
        options,
        source_info,
        pending_heredoc_trailing: &pending_heredoc_trailing,
    };
    let tracker = PositionTracker::new(input);
    let mut stream = LocatingSlice::new(input);
    program(&ctx, &tracker).parse_next(&mut stream)
}
