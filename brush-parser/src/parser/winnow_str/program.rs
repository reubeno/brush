use winnow::combinator::{opt, repeat, trace};
use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::stream::LocatingSlice;

use crate::ast;
use crate::parser::{ParserOptions, SourceInfo};

use super::and_or::and_or;
use super::helpers::{
    comment_tracking, linebreak_tracking, newline_list_tracking, separator_op, spaces_tracking,
};
use super::position::PositionTracker;
use super::types::{ParseContext, StrStream};

// ============================================================================
// Complete Commands and Programs
// ============================================================================

/// Parse a complete command (and/or lists with separators)
pub(super) fn complete_command<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::CompleteCommand, ContextError> + 'a {
    trace("complete_command", move |input: &mut StrStream<'a>| {
        let mut items: Vec<ast::CompoundListItem> = vec![];
        let mut ao = and_or(ctx, tracker).parse_next(input)?;
        loop {
            // Trailing spaces and inline comment after an and_or; track comments.
            spaces_tracking(ctx).parse_next(input)?;
            let Ok(sep) = separator_op().parse_next(input) else {
                items.push(ast::CompoundListItem(ao, ast::SeparatorOperator::Sequence));
                break;
            };
            spaces_tracking(ctx).parse_next(input)?;
            items.push(ast::CompoundListItem(ao, sep));
            match and_or(ctx, tracker).parse_next(input) {
                Ok(next) => ao = next,
                Err(_) => break,
            }
        }
        Ok(ast::CompoundList(items))
    })
}

/// Parse a newline-separated complete command continuation
fn complete_command_continuation<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::CompleteCommand, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        // newlines between statements may contain comment-only lines; track them.
        winnow::combinator::preceded(newline_list_tracking(ctx), complete_command(ctx, tracker))
            .parse_next(input)
    }
}

/// Parse multiple complete commands separated by newlines
pub(super) fn complete_commands<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, Vec<ast::CompleteCommand>, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        (
            complete_command(ctx, tracker),
            repeat::<_, _, Vec<_>, _, _>(0.., complete_command_continuation(ctx, tracker)),
        )
            .map(
                |(first, rest): (ast::CompleteCommand, Vec<ast::CompleteCommand>)| {
                    std::iter::once(first).chain(rest).collect()
                },
            )
            .parse_next(input)
    }
}

/// Parse a complete program
pub(super) fn program<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::Program, ContextError> + 'a {
    trace("program", move |input: &mut StrStream<'a>| {
        // Leading blank/comment lines before the first statement.
        linebreak_tracking(ctx).parse_next(input)?;
        let complete_commands = opt(complete_commands(ctx, tracker))
            .parse_next(input)?
            .unwrap_or_default();
        // Trailing blank/comment lines after the last statement.
        linebreak_tracking(ctx).parse_next(input)?;
        // A comment at the very end of a file without a trailing newline.
        let _: &str = super::helpers::opt_blanks().parse_next(input)?;
        opt(comment_tracking(ctx)).parse_next(input)?;

        // Anything left unparsed past valid commands means the whole input was
        // not consumed. Distinguish hard syntax errors (the leftover begins
        // with something that can never begin or continue a construct here,
        // e.g. `;;`, `)` or `fi`) from potentially-incomplete input (e.g. an
        // unclosed `if`, or a trailing `|` awaiting the next command): commit
        // to the former so it reports a real position, backtrack on the latter
        // so it keeps being reported as end-of-input.
        let leftover = input.as_ref().trim_end();

        if !leftover.is_empty()
            && starts_with_unrecoverable_token(leftover, complete_commands.is_empty())
        {
            return Err(winnow::error::ErrMode::Cut(ContextError::default()));
        }
        if !leftover.is_empty() {
            return Err(winnow::error::ErrMode::Backtrack(ContextError::default()));
        }

        // Convert accumulated byte ranges to SourceSpans.
        let comments = ctx
            .comments
            .borrow()
            .iter()
            .map(|r| tracker.range_to_span(r.clone()))
            .collect();

        Ok(ast::Program {
            complete_commands,
            comments,
        })
    })
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
) -> Result<ast::Program, crate::error::ParseError> {
    let _parse_state = super::parse_state::Parse::begin(options.tokenizer_options());
    let comments = std::cell::RefCell::new(Vec::new());
    let ctx = ParseContext {
        options,
        source_info,
        comments: &comments,
    };
    // Read the input as bash does: continuations dropped, left-open bodies moved in.
    let cleaned = super::continuations::remove(input)
        .map_err(|_| crate::error::ParseError::ParsingAtEndOfInput)?;
    let (text, tracker) = match cleaned {
        Some((text, anchors)) => {
            let tracker = PositionTracker::with_anchors(input, &text, anchors);
            (std::borrow::Cow::Owned(text), tracker)
        }
        None => (
            std::borrow::Cow::Borrowed(input),
            PositionTracker::new(input),
        ),
    };
    let mut stream = LocatingSlice::new(text.as_ref());
    let result = program(&ctx, &tracker).parse_next(&mut stream);
    result.map_err(|e| {
        use winnow::stream::Location;
        let offset = stream.current_token_start();
        match e {
            winnow::error::ErrMode::Cut(_) => {
                // Committed parse that failed - report the error position
                if offset >= text.len() {
                    crate::error::ParseError::ParsingAtEndOfInput
                } else {
                    let offset = tracker.input_offset(offset);
                    let (line, column) = calculate_line_column(input, offset);
                    crate::error::ParseError::ParsingNear(crate::SourcePosition {
                        index: offset,
                        line,
                        column,
                    })
                }
            }
            // Backtrack or Incomplete - might be incomplete input, signal "need more"
            winnow::error::ErrMode::Backtrack(_) | winnow::error::ErrMode::Incomplete(_) => {
                crate::error::ParseError::ParsingAtEndOfInput
            }
        }
    })
}

/// Returns whether the given unparsed trailing content starts with a token
/// that can never lead to a valid parse: a case terminator operator, a stray
/// closing delimiter/operator, a reserved word that only appears inside
/// another construct (`then`, `fi`, `done`, ...), or - when nothing has been
/// parsed yet - a binary operator like `|` or `&&`.
///
/// Operators like `|` and `&&` following already-parsed commands are NOT
/// unrecoverable: they legitimately continue onto the next line, so their
/// leftovers indicate incomplete input rather than an error.
fn starts_with_unrecoverable_token(rest: &str, nothing_parsed: bool) -> bool {
    /// Operator tokens that can never appear in a valid parse.
    const UNRECOVERABLE_OPS: &[&str] = &[";;&", ";;", ";&", ")"];
    /// Operator tokens that may continue a command onto the next line.
    const CONTINUATION_OPS: &[&str] = &["&&", "||", "|"];

    let rest = rest.trim_start_matches([' ', '\t', '\r', '\n']);

    // Quoted or escaped text can never be a reserved word or operator.
    if rest.starts_with(['\'', '"', '\\', '`', '$']) {
        return false;
    }

    if UNRECOVERABLE_OPS.iter().any(|op| rest.starts_with(op)) {
        return true;
    }

    if CONTINUATION_OPS.iter().any(|op| rest.starts_with(op)) {
        return nothing_parsed;
    }

    let mut word_end = rest.len();
    for (idx, c) in rest.char_indices() {
        if !(c.is_alphanumeric() || c == '_') {
            word_end = idx;
            break;
        }
    }
    let Some((word, remainder)) = rest.split_at_checked(word_end) else {
        return false;
    };

    if !matches!(
        word,
        "then" | "else" | "elif" | "fi" | "do" | "done" | "in" | "esac" | "}" | "]]"
    ) {
        return false;
    }

    // Only a delimiter may follow; `fi=x` and `esac()` are ordinary words.
    matches!(
        remainder.chars().next(),
        None | Some(' ' | '\t' | '\r' | '\n' | ';' | '&' | ')' | '|' | '<' | '>')
    )
}

fn calculate_line_column(input: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in input.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}
