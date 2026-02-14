use winnow::combinator::repeat;
use winnow::prelude::*;
use winnow::stream::LocatingSlice;

use crate::ast;
use crate::parser::{ParserOptions, SourceInfo};

use super::commands::command;
use super::helpers::{keyword, linebreak, spaces};
use super::position::PositionTracker;
use super::types::{PError, ParseContext, StrStream};

// ============================================================================
// Tier 4: Pipelines
// ============================================================================

/// Parse pipe operator ('|' or '|&')
/// Corresponds to: winnow.rs `pipe_operator()`
/// Returns true if it's |& (pipe stderr too)
#[inline]
pub(super) fn pipe_operator<'a>() -> impl Parser<StrStream<'a>, bool, PError> {
    // Note: Keep alt() for 2 alternatives - dispatch! is slower due to peek overhead
    winnow::combinator::alt((
        "|&".value(true), // |& pipes both stdout and stderr
        "|".value(false), // | pipes only stdout
    ))
}

/// Add stderr redirect (2>&1) to a command for |& support
fn add_pipe_extension_redirect(cmd: &mut ast::Command) {
    let redirect = ast::IoRedirect::File(
        Some(2), // stderr
        ast::IoFileRedirectKind::DuplicateOutput,
        ast::IoFileRedirectTarget::Fd(1), // redirect to stdout
    );

    match cmd {
        ast::Command::Simple(simple) => {
            let redirect_item = ast::CommandPrefixOrSuffixItem::IoRedirect(redirect);
            if let Some(suffix) = &mut simple.suffix {
                suffix.0.push(redirect_item);
            } else {
                simple.suffix = Some(ast::CommandSuffix(vec![redirect_item]));
            }
        }
        ast::Command::Compound(_, redirect_list) => {
            if let Some(list) = redirect_list {
                list.0.push(redirect);
            } else {
                *redirect_list = Some(ast::RedirectList(vec![redirect]));
            }
        }
        ast::Command::Function(func) => {
            if let Some(list) = &mut func.body.1 {
                list.0.push(redirect);
            } else {
                func.body.1 = Some(ast::RedirectList(vec![redirect]));
            }
        }
        ast::Command::ExtendedTest(_, rlist) => {
            // Add redirect to extended test
            if let Some(rlist) = rlist {
                rlist.0.push(redirect);
            } else {
                *rlist = Some(ast::RedirectList(vec![redirect]));
            }
        }
    }
}

/// Parse a single command from a string (used for trailing here-doc content)
fn parse_trailing_command(input: &str, options: &ParserOptions) -> Option<ast::Command> {
    let source_info = SourceInfo::default();
    let pending = std::cell::RefCell::new(None);
    let ctx = ParseContext {
        options,
        source_info: &source_info,
        pending_heredoc_trailing: &pending,
    };
    let tracker = PositionTracker::new(input);
    let mut stream = LocatingSlice::new(input);
    command(&ctx, &tracker).parse_next(&mut stream).ok()
}

/// Parse pipe sequence (command | command | command)
/// Corresponds to: winnow.rs `pipe_sequence()`
pub(super) fn pipe_sequence<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, Vec<ast::Command>, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let (first, rest) = (
            command(ctx, tracker),
            repeat::<_, _, Vec<_>, _, _>(
                0..,
                (
                    winnow::combinator::preceded(spaces(), pipe_operator()), // spaces then |
                    winnow::combinator::preceded((linebreak(), spaces()), command(ctx, tracker)), // optional newlines+spaces then command
                ),
            ),
        )
            .parse_next(input)?;

        // Build initial commands vector
        let mut commands =
            rest.into_iter()
                .fold(vec![first], |mut commands, (is_pipe_and, cmd)| {
                    if is_pipe_and {
                        // For |&, add 2>&1 redirect to the previous command
                        if let Some(prev_cmd) = commands.last_mut() {
                            add_pipe_extension_redirect(prev_cmd);
                        }
                    }
                    commands.push(cmd);
                    commands
                });

        // Check if there's pending trailing content from a here-doc (e.g., "| grep hello")
        if let Some(trailing) = ctx.pending_heredoc_trailing.borrow_mut().take() {
            // Parse the trailing content as additional pipeline commands
            if let Some(stripped) = trailing.strip_prefix('|') {
                let trailing_input = format!("{}\n", stripped.trim());
                if let Some(trailing_cmd) = parse_trailing_command(&trailing_input, ctx.options) {
                    commands.push(trailing_cmd);
                }
            }
        }

        Ok(commands)
    }
}

/// Parse optional time keyword with optional -p flag
/// Returns Option<PipelineTimed>
fn pipeline_timed<'a>(
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, Option<ast::PipelineTimed>, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        // Try to parse "time" keyword
        if keyword("time").parse_next(input).is_err() {
            return Ok(None);
        }

        // Consume spaces after "time"
        spaces().parse_next(input)?;

        // Check for optional "-p" flag
        let has_posix_flag =
            winnow::combinator::opt(winnow::combinator::terminated("-p", spaces()))
                .parse_next(input)?
                .is_some();

        let end_offset = tracker.offset_from_locating(input);
        let loc = tracker.range_to_span(start_offset..end_offset);

        let timed = if has_posix_flag {
            ast::PipelineTimed::TimedWithPosixOutput(loc)
        } else {
            ast::PipelineTimed::Timed(loc)
        };

        Ok(Some(timed))
    }
}

/// Parse optional bang (!) operators before a pipeline
/// Returns the count of bang operators
fn pipeline_bang<'a>() -> impl Parser<StrStream<'a>, usize, PError> {
    winnow::combinator::repeat(0.., winnow::combinator::terminated(keyword("!"), spaces()))
        .map(|bangs: Vec<_>| bangs.len())
}

/// Parse a pipeline
/// Corresponds to: winnow.rs `pipeline()` with full support for time and bang
pub(super) fn pipeline<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl Parser<StrStream<'a>, ast::Pipeline, PError> + 'a {
    move |input: &mut StrStream<'a>| {
        (
            pipeline_timed(tracker),
            pipeline_bang(),
            pipe_sequence(ctx, tracker),
        )
            .map(|(timed, bang_count, seq)| {
                ast::Pipeline {
                    timed,
                    bang: bang_count % 2 == 1, // Odd number of bangs = inverted
                    seq,
                }
            })
            .parse_next(input)
    }
}
