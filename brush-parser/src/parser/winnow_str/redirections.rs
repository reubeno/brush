use winnow::combinator::{dispatch, fail};
use winnow::error::ContextError;
use winnow::prelude::*;
use winnow::stream::{Location, Stream};

use crate::ast;

use super::compound::process_substitution;
use super::helpers::{peek_op2, spaces};
use super::here_doc_bodies;
use super::position::PositionTracker;
use super::types::{ParseContext, StrStream};
use super::words::word_as_ast;

// ============================================================================
// Tier 8: Redirections
// ============================================================================

/// Parse an I/O file descriptor number
pub(super) fn io_number<'a>() -> impl ModalParser<StrStream<'a>, i32, ContextError> {
    winnow::ascii::dec_uint::<_, u16, _>.map(i32::from)
}

/// Parse redirect operator and return the redirect kind
/// Corresponds to: winnow.rs `io_file()` dispatcher
fn redirect_operator<'a>() -> impl ModalParser<StrStream<'a>, ast::IoFileRedirectKind, ContextError>
{
    dispatch! {peek_op2();
        ">>" => ">>".value(ast::IoFileRedirectKind::Append),
        "<>" => "<>".value(ast::IoFileRedirectKind::ReadAndWrite),
        ">|" => ">|".value(ast::IoFileRedirectKind::Clobber),
        ">&" => ">&".value(ast::IoFileRedirectKind::DuplicateOutput),
        "<&" => "<&".value(ast::IoFileRedirectKind::DuplicateInput),
        ">" => ">".value(ast::IoFileRedirectKind::Write),
        "<" => "<".value(ast::IoFileRedirectKind::Read),
        _ => fail,
    }
}

/// Parse a here-document delimiter, handling quotes
/// Returns (`delimiter_text`, `requires_expansion`)
/// Returns (`raw_delimiter`, `match_delimiter`, `requires_expansion`)
/// `raw_delimiter`: as written (includes quotes for `here_end`)
/// `match_delimiter`: stripped of quotes (for matching content)
fn here_document_delimiter<'a>()
-> impl ModalParser<StrStream<'a>, (String, String, bool), ContextError> {
    move |input: &mut StrStream<'a>| {
        let mut raw_delimiter = String::new();
        let mut match_delimiter = String::new();
        let mut quoted = false;
        let mut open_quote = None;
        let mut done = false;

        while !done && !input.is_empty() {
            let checkpoint = input.checkpoint();

            // Whitespace ends the delimiter, and so does an operator character
            // outside quotes: `cat <<EOF; next`, `cat <<EOF|next`, `(cat <<EOF)`.
            if let Ok(ch) = winnow::token::any::<_, ContextError>.parse_next(input) {
                input.reset(&checkpoint);
                let operator = matches!(ch, ';' | '&' | '|' | '<' | '>' | '(' | ')');
                if matches!(ch, ' ' | '\t' | '\n') || (operator && open_quote.is_none()) {
                    break;
                }
            }

            // Try to parse a character
            let ch: char = winnow::token::any.parse_next(input)?;
            raw_delimiter.push(ch);

            match ch {
                '\'' | '"' => {
                    quoted = true;
                    // Don't include quotes in match delimiter
                    match open_quote {
                        None => open_quote = Some(ch),
                        Some(open) if open == ch => open_quote = None,
                        Some(_) => match_delimiter.push(ch),
                    }
                }
                '\\' => {
                    quoted = true;
                    // Consume next character
                    if let Ok(next_ch) = winnow::token::any::<_, ContextError>.parse_next(input) {
                        raw_delimiter.push(next_ch);
                        match_delimiter.push(next_ch);
                    }
                }
                ' ' | '\t' | '\n' => {
                    // End of delimiter
                    done = true;
                }
                _ => {
                    match_delimiter.push(ch);
                }
            }
        }

        if match_delimiter.is_empty() {
            return fail.parse_next(input);
        }

        let requires_expansion = !quoted;
        Ok((raw_delimiter, match_delimiter, requires_expansion))
    }
}

/// Parse here-document content until delimiter is found
/// Returns the content as a Word
fn here_document_content(
    input: &mut StrStream<'_>,
    delimiter: &str,
    remove_tabs: bool,
    tracker: &PositionTracker,
) -> ModalResult<ast::Word> {
    let start_offset = tracker.offset_from_locating(input);
    let mut content = String::new();
    let mut at_line_start = true;

    loop {
        // Check if we're at a line that matches the delimiter
        if at_line_start {
            let checkpoint = input.checkpoint();

            // Skip leading tabs if remove_tabs is true (for both delimiter and content)
            if remove_tabs {
                let _: ModalResult<&str> = winnow::token::take_while(0.., '\t').parse_next(input);
            }

            // Try to match delimiter
            if let Ok(line_content) =
                winnow::token::take_while::<_, _, ContextError>(0.., |c| c != '\n')
                    .parse_next(input)
            {
                if line_content == delimiter {
                    // Do NOT consume the newline after the delimiter — it serves
                    // as the command separator so that complete_command_continuation
                    // can find the next command on the following line.
                    let end_offset = tracker.offset_from_locating(input);
                    let loc = tracker.range_to_span(start_offset..end_offset);
                    return Ok(ast::Word {
                        value: content,
                        loc: Some(loc),
                    });
                }
            }

            // Not the delimiter, reset to get full line
            input.reset(&checkpoint);

            // If remove_tabs, skip leading tabs from content too
            if remove_tabs {
                let _: ModalResult<&str> = winnow::token::take_while(0.., '\t').parse_next(input);
            }
        }

        // Collect this line's content
        at_line_start = false;

        if input.is_empty() {
            // Unterminated here-document
            return fail.parse_next(input);
        }

        let ch: char = winnow::token::any.parse_next(input)?;
        content.push(ch);

        if ch == '\n' {
            at_line_start = true;
        }
    }
}

/// A here-document whose operator and delimiter are parsed and whose body is
/// not yet read
#[derive(Debug)]
struct PendingHereDoc {
    fd: Option<i32>,
    remove_tabs: bool,
    requires_expansion: bool,
    raw_delimiter: String,
    match_delimiter: String,
}

/// Parse just the here-document marker (operator and delimiter), without consuming content.
/// This is used to collect all markers on a line before resolving content.
fn here_document_marker<'a>() -> impl ModalParser<StrStream<'a>, PendingHereDoc, ContextError> + 'a
{
    move |input: &mut StrStream<'a>| {
        // Optional fd number
        let fd = winnow::combinator::opt(io_number()).parse_next(input)?;

        // Parse operator (<<- or <<)
        let remove_tabs =
            winnow::combinator::alt(("<<-".value(true), "<<".value(false))).parse_next(input)?;

        // Skip optional spaces between operator and delimiter (e.g., <<- EOF)
        let _: &str =
            winnow::token::take_while(0.., |c: char| c == ' ' || c == '\t').parse_next(input)?;

        // Parse delimiter - raw_delimiter preserves quotes, match_delimiter is stripped
        let (raw_delimiter, match_delimiter, requires_expansion) =
            here_document_delimiter().parse_next(input)?;

        Ok(PendingHereDoc {
            fd,
            remove_tabs,
            requires_expansion,
            raw_delimiter,
            match_delimiter,
        })
    }
}

/// Resolve a pending here-document by parsing its content from the input.
fn resolve_here_document(
    input: &mut StrStream<'_>,
    pending: PendingHereDoc,
    tracker: &PositionTracker,
) -> ModalResult<(Option<i32>, ast::IoHereDocument)> {
    let doc = here_document_content(
        input,
        &pending.match_delimiter,
        pending.remove_tabs,
        tracker,
    )?;

    Ok((
        pending.fd,
        ast::IoHereDocument {
            remove_tabs: pending.remove_tabs,
            requires_expansion: pending.requires_expansion,
            here_end: ast::Word::from(pending.raw_delimiter),
            doc,
        },
    ))
}

/// Parse a here-document redirect: its operator and delimiter here, and its
/// body from the lines after this one
///
/// The rest of the operator's line is left for the grammar to go on with.
/// The body is read ahead and its lines are recorded, to be stepped over when
/// the line's newline is consumed (see [`here_doc_bodies`]).
fn here_document<'a>(
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, (Option<i32>, ast::IoHereDocument), ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        let operator = input.current_token_start();
        let marker = here_document_marker().parse_next(input)?;

        // Look ahead on a copy: the end of this line, then past the bodies of
        // the here-documents before this one on the line.
        let mut ahead = *input;
        let _: &str = winnow::token::take_while(0.., |c| c != '\n').parse_next(&mut ahead)?;
        let _: char = '\n'.parse_next(&mut ahead)?;
        let after_line = ahead.current_token_start();
        let start = here_doc_bodies::body_start(operator, after_line);
        let _ = ahead.next_slice(start - after_line);

        let resolved = resolve_here_document(&mut ahead, marker, tracker)?;
        // The delimiter's own newline belongs to the body's lines.
        let _: ModalResult<char> = '\n'.parse_next(&mut ahead);
        here_doc_bodies::claim(operator, after_line, ahead.current_token_start());
        Ok(resolved)
    }
}

/// Result of parsing an I/O redirect
pub(super) struct IoRedirectResult {
    /// The parsed redirect
    pub redirect: ast::IoRedirect,
}

/// Parse a file redirect (e.g., "> file", "2>&1", "< input")
/// Corresponds to: winnow.rs `io_file()` + `io_redirect()`
pub(super) fn io_redirect<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, IoRedirectResult, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        winnow::combinator::alt((
            // Try OutputAndError redirects first (&>> and &>)
            (
                "&>>",
                winnow::combinator::preceded(spaces(), word_as_ast(ctx, tracker)),
            )
                .map(|(_, target)| IoRedirectResult {
                    redirect: ast::IoRedirect::OutputAndError(target, true),
                }),
            (
                "&>",
                winnow::combinator::preceded(spaces(), word_as_ast(ctx, tracker)),
            )
                .map(|(_, target)| IoRedirectResult {
                    redirect: ast::IoRedirect::OutputAndError(target, false),
                }),
            // Try here-string (<<<)
            (
                winnow::combinator::opt(io_number()),
                "<<<",
                winnow::combinator::preceded(spaces(), word_as_ast(ctx, tracker)),
            )
                .map(|(fd, _, word)| IoRedirectResult {
                    redirect: ast::IoRedirect::HereString(fd, word),
                }),
            // Try here-document
            here_document(tracker).map(|(fd, here_doc)| IoRedirectResult {
                redirect: ast::IoRedirect::HereDocument(fd, here_doc),
            }),
            // Then try regular file redirects (including process substitution as target)
            move |input: &mut StrStream<'a>| {
                let fd = winnow::combinator::opt(io_number()).parse_next(input)?;
                let kind = redirect_operator().parse_next(input)?;
                spaces().parse_next(input)?;

                // Try process substitution as redirect target first (e.g., < <(cmd))
                let redirect_target = if let Ok((ps_kind, ps_cmd)) =
                    process_substitution(ctx, tracker).parse_next(input)
                {
                    ast::IoFileRedirectTarget::ProcessSubstitution(ps_kind, ps_cmd)
                } else {
                    let target = word_as_ast(ctx, tracker).parse_next(input)?;
                    match kind {
                        ast::IoFileRedirectKind::DuplicateOutput
                        | ast::IoFileRedirectKind::DuplicateInput => {
                            ast::IoFileRedirectTarget::Duplicate(target)
                        }
                        _ => ast::IoFileRedirectTarget::Filename(target),
                    }
                };

                Ok(IoRedirectResult {
                    redirect: ast::IoRedirect::File(fd, kind, redirect_target),
                })
            },
        ))
        .parse_next(input)
    }
}

/// Parse a redirect list (one or more redirects)
/// Corresponds to: winnow.rs `redirect_list()`
pub(super) fn redirect_list<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, ast::RedirectList, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        winnow::combinator::repeat::<_, _, Vec<_>, _, _>(
            1..,
            winnow::combinator::preceded(spaces(), io_redirect(ctx, tracker)).map(|r| r.redirect),
        )
        .map(ast::RedirectList)
        .parse_next(input)
    }
}

/// Helper: Parse optional redirects after a compound command
/// Optimized to peek for redirect operators before attempting parse
pub(super) fn optional_redirects<'a>(
    ctx: &'a ParseContext<'a>,
    tracker: &'a PositionTracker,
) -> impl ModalParser<StrStream<'a>, Option<ast::RedirectList>, ContextError> + 'a {
    move |input: &mut StrStream<'a>| {
        // First, consume spaces and line continuations to see what follows
        super::helpers::spaces().parse_next(input)?;

        // Check if next char is a redirect operator or digit (for fd redirects like 2>)
        let has_redirect = input
            .as_ref()
            .chars()
            .next()
            .is_some_and(|c| c == '<' || c == '>' || c.is_ascii_digit());

        if has_redirect {
            winnow::combinator::opt(redirect_list(ctx, tracker)).parse_next(input)
        } else {
            Ok(None)
        }
    }
}
