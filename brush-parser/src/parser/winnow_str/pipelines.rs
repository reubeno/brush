use winnow::combinator::{fail, repeat};
use winnow::error::ContextError;
use winnow::prelude::*;

use crate::ast;

use super::commands::command;
use super::helpers::{keyword, linebreak, spaces};
use super::position::PositionTracker;
use super::types::{ParseContext, StrStream};

// ============================================================================
// Tier 4: Pipelines
// ============================================================================

/// Parse pipe operator ('|' or '|&')
/// Corresponds to: winnow.rs `pipe_operator()`
/// Returns true if it's |& (pipe stderr too)
#[inline]
pub(super) fn pipe_operator<'a>() -> impl ModalParser<StrStream<'a>, bool, ContextError> {
    // Note: Keep alt() for 2 alternatives - dispatch! is slower due to peek overhead
    winnow::combinator::alt((
        "|&".value(true), // |& pipes both stdout and stderr
        "|".value(false), // | pipes only stdout
    ))
}

/// Add stderr redirect (2>&1) to a command for |& support
fn add_pipe_extension_redirect(cmd: &mut ast::Command) {
    add_redirect_to_command(
        cmd,
        ast::IoRedirect::File(
            Some(2), // stderr
            ast::IoFileRedirectKind::DuplicateOutput,
            ast::IoFileRedirectTarget::Fd(1), // redirect to stdout
        ),
    );
}

/// Append a redirect to a command's redirect list / suffix.
fn add_redirect_to_command(cmd: &mut ast::Command, redirect: ast::IoRedirect) {
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
    }
}

/// Parse pipe sequence (command | command | command)
/// Corresponds to: winnow.rs `pipe_sequence()`
pub(super) fn pipe_sequence<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, Vec<ast::Command>, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let (first, rest) =
            (
                command(ctx, tracker),
                repeat::<_, _, Vec<_>, _, _>(
                    0..,
                    (
                        winnow::combinator::preceded(spaces(), pipe_operator()), // spaces then |
                        winnow::combinator::preceded(
                            (linebreak(), spaces()),
                            command(ctx, tracker),
                        ), /* optional newlines+spaces then command */
                    ),
                ),
            )
                .parse_next(input)?;

        // Build initial commands vector
        let commands = rest
            .into_iter()
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

        Ok(commands)
    }
}

/// Parse optional time keyword with optional -p flag
/// Returns Option<PipelineTimed>
fn pipeline_timed<'a>(
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, Option<ast::PipelineTimed>, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let start_offset = tracker.offset_from_locating(input);

        // Try to parse "time" keyword
        if winnow::combinator::opt(keyword("time"))
            .parse_next(input)?
            .is_none()
        {
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
fn pipeline_bang<'a>() -> impl ModalParser<StrStream<'a>, usize, ContextError> {
    winnow::combinator::repeat(0.., winnow::combinator::terminated(keyword("!"), spaces()))
        .map(|bangs: Vec<_>| bangs.len())
}

/// Parse a pipeline
/// Corresponds to: winnow.rs `pipeline()` with full support for time and bang
pub(super) fn pipeline<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::Pipeline, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let (timed, bang_count) = (pipeline_timed(tracker), pipeline_bang()).parse_next(input)?;

        // pipe_sequence is optional - it may fail if there's no command
        // (e.g., standalone '!' or 'time')
        let seq = winnow::combinator::opt(pipe_sequence(ctx, tracker))
            .parse_next(input)?
            .unwrap_or_default();

        // Validate: at least one of timed, bang, or seq must be present
        if timed.is_none() && bang_count == 0 && seq.is_empty() {
            return fail.parse_next(input);
        }

        Ok(ast::Pipeline {
            timed,
            bang: bang_count % 2 == 1,
            seq,
        })
    }
}
