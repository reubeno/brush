use crate::ast;

use winnow::{
    ascii::{digit1, line_ending},
    combinator::{
        alt, cut_err, delimited, empty, fail, opt, peek, preceded, repeat, repeat_till, terminated,
        trace,
    },
    dispatch,
    error::ContextError,
    token::{any, literal, one_of, take},
    PResult, Parser,
};

use super::{
    custom_combinators::non_posix_extension,
    trivia::{comment, space, trim_whitespace},
    word, Input,
};

pub fn io_redirect(i: &mut Input<'_>) -> PResult<ast::IoRedirect> {
    trace(
        "io_redirect",
        alt((
            |i: &mut Input<'_>| {
                let n = opt(io_number).parse_next(i)?;
                alt((
                    io_file.map(move |(kind, target)| ast::IoRedirect::File(n, kind, target)),
                    io_here.map(move |h| ast::IoRedirect::HereDocument(n, h)),
                    non_posix_extension(("<<<", preceded(space(0..), cut_err(word::word)))).map(
                        move |(_, w)| {
                            ast::IoRedirect::HereString(n, ast::Word::from(w.to_string()))
                        },
                    ),
                ))
                .parse_next(i)
            },
            non_posix_extension(
                alt((
                    ("&>>".value(true), preceded(space(0..), cut_err(word::word))),
                    ("&>".value(false), preceded(space(0..), cut_err(word::word))),
                ))
                .map(|(append, f)| {
                    ast::IoRedirect::OutputAndError(ast::Word::from(f.to_string()), append)
                }),
            ),
        )),
    )
    .parse_next(i)
}

pub fn redirect_list(i: &mut Input<'_>) -> PResult<ast::RedirectList> {
    repeat(1.., terminated(io_redirect, space(0..)))
        .map(ast::RedirectList)
        .parse_next(i)
}

fn io_fd(i: &mut Input<'_>) -> PResult<u32> {
    trace("fd", digit1.parse_to()).parse_next(i)
}

fn io_number(i: &mut Input<'_>) -> PResult<u32> {
    // N.B. An I/O number must be a string of only digits, and it must be
    // followed by a '<' or '>' character (but not consume them).
    trace("io_number", terminated(io_fd, peek(one_of((b'<', b'>'))))).parse_next(i)
}

fn io_here(i: &mut Input<'_>) -> PResult<ast::IoHereDocument> {
    trace("io_here", |i: &mut Input<'_>| {
        let remove_tabs =
            alt((literal("<<-").value(true), literal("<<").value(false))).parse_next(i)?;
        let (tag, requires_expansion) = delimited(
            space(0..),
            cut_err(word::word).with_taken().map(|(tag, raw_tag)| {
                // from the manual:
                // > No parameter expansion, command substitution, arithmetic expansion, or pathname
                // > expansion is performed on word. If any characters in word are quoted
                let requires_expansion = !raw_tag
                    .into_iter()
                    .any(|c| *c == b'"' || *c == b'\'' || *c == b'\\');
                (tag, requires_expansion)
            }),
            // TODO: line_space or what
            (space(0..), opt(comment), line_ending),
        )
        .parse_next(i)?;
        let r = cut_err(
            repeat_till(0.., any, (line_ending, tag.as_bytes()))
                .map(|((), _)| ())
                .take(),
        )
        .try_map(std::str::from_utf8)
        .map(|doc| ast::IoHereDocument {
            remove_tabs,
            requires_expansion,
            here_end: ast::Word::from(tag.to_string()),
            doc: ast::Word::from(doc.to_string()),
        })
        .parse_next(i);
        r
    })
    .parse_next(i)
}

fn io_file<'i>(i: &mut Input<'i>) -> PResult<(ast::IoFileRedirectKind, ast::IoFileRedirectTarget)> {
    trace("io_file", alt((
            preceded(('<', space(0..)), io_filename).map(|f| (ast::IoFileRedirectKind::Write, f)),
            preceded(('>', space(0..)), io_filename).map(|f| (ast::IoFileRedirectKind::Read, f)),
            (dispatch! {take::<_, Input<'i>, _>(2usize);
                b">>" => preceded(space(0..), cut_err(io_filename)).map(|f| (ast::IoFileRedirectKind::Append, f)),
                b"<&" => preceded(space(0..), cut_err(filename_or_fd)).map(|f| (ast::IoFileRedirectKind::DuplicateInput, f)),
                b">&" => preceded(space(0..), cut_err(filename_or_fd)).map(|f| (ast::IoFileRedirectKind::DuplicateOutput, f)),
                b"<>" => preceded(space(0..), cut_err(io_filename)).map(|f| (ast::IoFileRedirectKind::ReadAndWrite, f)),
                b">|" => preceded(space(0..), cut_err(io_filename)).map(|f| (ast::IoFileRedirectKind::Clobber, f)),
                _ => fail
            })
        )))
           .parse_next(i)
}

fn io_filename(i: &mut Input<'_>) -> PResult<ast::IoFileRedirectTarget, ContextError> {
    trace(
        "io_filename",
        alt((
            // N.B. Process substitution forms are extensions to the POSIX standard.
            non_posix_extension(process_substitution).map(|(kind, subshell)| {
                ast::IoFileRedirectTarget::ProcessSubstitution(kind, subshell)
            }),
            word::word.map(|f| ast::IoFileRedirectTarget::Filename(ast::Word::from(f.to_string()))),
        )),
    )
    .parse_next(i)
}

fn filename_or_fd(i: &mut Input<'_>) -> PResult<ast::IoFileRedirectTarget> {
    trace(
        "io_filename_or_fd",
        alt((io_fd.map(ast::IoFileRedirectTarget::Fd), io_filename)),
    )
    .parse_next(i)
}

pub fn process_substitution(
    i: &mut Input<'_>,
) -> PResult<(ast::ProcessSubstitutionKind, ast::SubshellCommand)> {
    use super::complete_command::compound_list;
    trace(
        "process_substitution",
        (
            dispatch! {peek(any);
                b'<' => empty.value(ast::ProcessSubstitutionKind::Read),
                b'>' => empty.value(ast::ProcessSubstitutionKind::Write),
                _ => fail,
            },
            preceded(
                "(",
                cut_err(terminated(trim_whitespace(0.., compound_list, 0..), ")")),
            )
            .map(ast::SubshellCommand),
        ),
    )
    .parse_next(i)
}

#[cfg(test)]
mod tests {
    use crate::parser2::new_input;
    use crate::ParserOptions;

    use super::*;

    #[test]
    fn parse_heredoc() {
        fn parse<'i>(i: &'i str) {
            let io = io_here
                .parse_next(&mut new_input(ParserOptions::default(), i))
                .unwrap();
            dbg!(io);
        }
        parse(
            r#"<<EOF
aa
EOF
"#,
        )
    }
    #[test]
    fn parse_empty_process_substitution() {
        fn parse<'i>(i: &'i str) {
            let io = process_substitution
                .parse_next(&mut new_input(ParserOptions::default(), i))
                .unwrap();
            dbg!(io);
        }
        parse(r#"<()"#)
    }
}
