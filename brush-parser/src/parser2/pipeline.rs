use crate::ast;

use winnow::{
    combinator::{cut_err, delimited, fail, opt, peek, repeat, separated_pair, trace},
    dispatch,
    stream::Accumulate,
    token::take,
    PResult, Parser,
};

use super::{
    complete_command::command,
    trivia::{line_trailing, space},
    Input,
};

/// `pipeline := bang? pipe_sequence`
///```yacc
/// pipeline         :      pipe_sequence
///                  | Bang pipe_sequence
/// ```
pub fn pipeline(i: &mut Input<'_>) -> PResult<ast::Pipeline> {
    trace(
        "pipeline",
        separated_pair(
            opt("!").map(|bang| bang.is_some()),
            space(0..),
            pipe_sequence,
        ),
    )
    .map(|(bang, seq)| ast::Pipeline { bang, seq })
    .parse_next(i)
}

/// ```yacc
/// pipe_sequence    :                             command
///                  | pipe_sequence '|' linebreak command
/// ```
/// `pipe_sequence := command | (command (pipe_operator line_trailing* command)*)`
pub fn pipe_sequence(i: &mut Input<'_>) -> PResult<Vec<ast::Command>> {
    trace("pipe_sequence", |i: &mut Input<'_>| {
        let mut current = trace("first_command", command).parse_next(i)?;

        let pipe = delimited(
            space(0..),
            pipe_operator,
            (opt(line_trailing), space(0..)),
        );
        let r = trace(
            "remaining_pipe_sequence",
            opt(
                repeat(1.., (pipe, cut_err(command))).fold(Vec::new, |mut acc, (p, c)| {
                    if matches!(p, PipeOperator::Extension) {
                        add_pipe_extension_redirection(&mut current);
                    }
                    acc.accumulate(std::mem::replace(&mut current, c));
                    acc
                }),
            ),
        )
        .parse_next(i)?;
        Ok(r.unwrap_or_else(|| vec![current]))
    })
    .parse_next(i)
}

#[derive(Clone, Copy)]
enum PipeOperator {
    Simple,
    Extension,
}

/// ` pipe-operator := !'||' ('|&' | '|') `
fn pipe_operator(i: &mut Input<'_>) -> PResult<PipeOperator> {
    trace(
        "pipe_operator",
        dispatch!(peek::<_, &[u8],_,_>(take(2usize));
            b"||" => fail,
            b"|&" => take(2usize).value(PipeOperator::Extension),
            _ => "|".value(PipeOperator::Simple),
        ),
    )
    .parse_next(i)
}

// add `2>&1` to the command if the pipeline is `|&`
pub fn add_pipe_extension_redirection(c: &mut ast::Command) {
    let r = ast::IoRedirect::File(
        Some(2),
        ast::IoFileRedirectKind::DuplicateOutput,
        ast::IoFileRedirectTarget::Fd(1),
    );

    fn add_to_redirect_list(l: &mut Option<ast::RedirectList>, r: ast::IoRedirect) {
        if let Some(l) = l {
            l.0.push(r);
        } else {
            let v = vec![r];
            *l = Some(ast::RedirectList(v));
        }
    }

    match c {
        ast::Command::Simple(c) => {
            let r = ast::CommandPrefixOrSuffixItem::IoRedirect(r);
            if let Some(l) = &mut c.suffix {
                l.0.push(r);
            } else {
                c.suffix = Some(ast::CommandSuffix(vec![r]));
            }
        }
        ast::Command::Compound(_, l) => add_to_redirect_list(l, r),
        ast::Command::Function(f) => add_to_redirect_list(&mut f.body.1, r),
        // TODO: redirect_list for extended tests
        ast::Command::ExtendedTest(_) => (),
    };
}
