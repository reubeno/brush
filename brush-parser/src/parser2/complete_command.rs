use winnow::{
    ascii::line_ending,
    combinator::{
        alt, cut_err, dispatch, eof, fail, not, opt, peek, preceded, repeat, terminated, trace,
    },
    prelude::*,
    stream::Stream as _,
    token::{any, take},
    PResult,
};

pub mod compound_command;
mod extended_test;
mod function_definition;
mod simple_command;

use crate::{
    ast::{self, SeparatorOperator},
    parser2::trivia::line_trailing,
};

use super::{
    custom_combinators::{self, non_posix_extension},
    io::redirect_list,
    pipeline,
    trivia::{self, comment, line_space, space, trim_whitespace},
    Input,
};

pub(crate) fn insignificant<'i>(i: &mut Input<'i>) -> PResult<()> {
    trace("newline_list", move |i: &mut Input<'i>| {
        while i.eof_offset() > 0 {
            match peek(any).parse_next(i)? {
                b' ' => space(1..).parse_next(i)?,
                trivia::LF | trivia::CR => line_ending.void().parse_next(i)?,
                trivia::COMMENT => (comment, alt((line_ending, eof))).void().parse_next(i)?,
                _ => break,
            }
        }
        Ok(())
    })
    .parse_next(i)
}

// compound_list    : linebreak term
//                  | linebreak term separator
//                  ;
// term             : term separator and_or
//                  |                and_or

// complete_commands: complete_commands newline_list complete_command
//                  |                                complete_command
//                  ;
// complete_command : list separator_op
//                  | list
//                  ;
// list             : list separator_op and_or
//                  |                   and_or

// separator_op     : '&'
//                  | ';'
//                  ;
// separator        : separator_op linebreak
//                  | newline_list

// newline_list     :              NEWLINE
//                  | newline_list NEWLINE
//                  ;
// linebreak        : newline_list
// | /* empty */
// TODO: possibly smallvec
type CompleteCommand = Vec<ast::CompoundListItem>;

// [     echo hello && true   ; echo world || false ;    #comment \n]
// [echo hello && :\n]
pub(crate) fn complete_command(i: &mut Input<'_>) -> PResult<CompleteCommand> {
    trace(
        "complete_command",
        repeat(
            1..,
            preceded(
                // N.B emulate `repeat_till` but instead return accumulated result on
                // backtrack. because if `repeat_till` fails, it discards
                // all the accumulated output...
                not(line_ending),
                (
                    and_or,
                    // `;` `&` or the end of the line
                    alt((
                        trim_whitespace(0.., separator_op, 0..),
                        line_space.value(SeparatorOperator::default()),
                    )),
                )
                    .map(|(ao, sep)| ast::CompoundListItem(ao, sep)),
            ),
        ),
    )
    .parse_next(i)
}

/// `compound-list`
/// it is equivalent to a sequence of lists, separated by <newline> characters, that can be preceded
/// or followed by an arbitrary number of <newline> characters.

/// https://pubs.opengroup.org/onlinepubs/9799919799/utilities/V3_chap02.html 2.9.3 Lists
/// A list is a sequence of one or more AND-OR lists separated by the operators ';' and '&'.
/// `complete-command := (and-or whitespace* (';' | '&') whitespace*)+`
/// A sequence of commands
/// `compound-list := (and-or ((separator-op | line-trailing) line-trailing*))*`
pub fn compound_list(i: &mut Input<'_>) -> PResult<ast::CompoundList> {
    trace(
        "compound_list",
        preceded(
            insignificant,
            repeat(0.., terminated(complete_command, insignificant))
                .fold(Vec::new, |mut acc, c| {
                    acc.extend(c.into_iter());
                    acc
                })
                // N.B: An empty compound list doesn't allowed by the Posix spec
                // See: https://unix.stackexchange.com/questions/349632/can-a-function-in-sh-have-zero-statements
                // A portable posix compliant script will always needs to provide a non-empty
                // `compound_list`.
                // TODO: error context explanation
                .verify(|l: &Vec<_>| !l.is_empty()),
        ),
    )
    .map(ast::CompoundList)
    .parse_next(i)
}

/// `separator-op := !(';;' | '&&') ';' | '&' `
pub fn separator_op(i: &mut Input<'_>) -> PResult<ast::SeparatorOperator> {
    trace(
        "separator_op",
        preceded(
            // it is case-clause. (or an empty command, but it is not allowed to have and empty
            // complete_command)
            not(alt((";;", ";&", "&&"))),
            alt((
                b';'.value(ast::SeparatorOperator::Sequence),
                b'&'.value(ast::SeparatorOperator::Async),
            )),
        ),
    )
    .parse_next(i)
}

/// https://pubs.opengroup.org/onlinepubs/9799919799/utilities/V3_chap02.html
/// 2.9.3 Lists
/// An AND-OR list is a sequence of one or more pipelines separated by the operators "&&" and "||".
/// `and-or := pipeline`
pub fn and_or(i: &mut Input<'_>) -> PResult<ast::AndOrList> {
    trace("and_or", (pipeline::pipeline, and_or_items))
        .map(|(first, additional)| ast::AndOrList { first, additional })
        .parse_next(i)
}

/// `and-or-items := ( ('&&' | '||') line-trailing* pipeline)*`
fn and_or_items(i: &mut Input<'_>) -> PResult<Vec<ast::AndOr>> {
    trace(
        "and_or_items",
        repeat(
            0..,
            dispatch!(take::<_, Input<'_>, _>(2u8);
                // `line-trailing` indicates that:
                // ```
                //  echo hello [&& # my comment
                //      ]echo world
                // ```
                b"&&" => preceded((line_trailing, space(0..)), cut_err(pipeline::pipeline)).map(ast::AndOr::And),
                b"||" => preceded((line_trailing, space(0..)), cut_err(pipeline::pipeline)).map(ast::AndOr::Or),
                _ => fail
            ),
        ),
    )
    .parse_next(i)
}

/// command := simple-command | function-definition | compound-command | extended-test
pub fn command(i: &mut Input<'_>) -> PResult<ast::Command> {
    trace(
        "command",
        alt((
            simple_command::simple_command.map(ast::Command::Simple),
            function_definition::function_definition.map(ast::Command::Function),
            (
                compound_command::compound_command,
                opt(preceded(space(0..), redirect_list)),
            )
                .map(|(c, r)| ast::Command::Compound(c, r)),
            // N.B. Extended test commands are bash extensions.
            non_posix_extension(extended_test::extended_test_command)
                .map(ast::Command::ExtendedTest),
        )),
    )
    .parse_next(i)
}
