/// 2.9.4 Compound Commands https://pubs.opengroup.org/onlinepubs/9799919799/utilities/V3_chap02.html
use winnow::{
    ascii::line_ending,
    combinator::{alt, cut_err, delimited, opt, preceded, repeat, separated, terminated, trace},
    error::ContextError,
    prelude::*,
    PResult,
};

use crate::{
    ast::{self},
    parser2::{
        custom_combinators::non_posix_extension,
        trivia::{line_trailing, space, trim_whitespace},
        word, Input,
    },
};

use self::word::space_after_word;
use super::compound_list;

/// `compound-command := brace-group | arithmetic-command | subshell | if-clause | for-clause |
/// while-or-until-clause | arithmetic-for-clause`
pub fn compound_command(i: &mut Input<'_>) -> PResult<ast::CompoundCommand, ContextError> {
    trace(
        "compound_command",
        alt((
            brace_group.map(ast::CompoundCommand::BraceGroup),
            // N.B. The arithmetic command is a non-sh extension.
            non_posix_extension(arithmetics::arithmetic_command)
                .map(ast::CompoundCommand::Arithmetic),
            subshell.map(ast::CompoundCommand::Subshell),
            if_clause::if_clause.map(ast::CompoundCommand::IfClause),
            for_clause::for_clause.map(ast::CompoundCommand::ForClause),
            while_or_until_clause,
            case_clause::case_clause.map(ast::CompoundCommand::CaseClause),
            // N.B. The arithmetic for clause command is a non-sh extension.
            non_posix_extension(arithmetics::arithmetic_for_clause)
                .map(ast::CompoundCommand::ArithmeticForClause),
        )),
    )
    .parse_next(i)
}

/// `brace-group := '{' (line-trailing | whitespace) whitespace* compound-list whitespace* '}'`
fn brace_group(i: &mut Input<'_>) -> PResult<ast::BraceGroupCommand> {
    trace(
        "brace_group",
        bracketed("{", compound_list, "}").map(ast::BraceGroupCommand),
    )
    .parse_next(i)
}

// TODO: somehow stop parsing earlier
// because for now the `compound_list` unsuccessfuly tries to parse `done` and unable to do so
// checks everything down the the grammar:(

/// A combinator that encapsulates something inside brackets and tailored for the bash grammar
/// `bracketed := bracket1 (line-trailing | whitespace) whitespace* parser whitespace* bracket2`
fn bracketed<'i, Output, Bracket1, I1, Bracket2, I2>(
    bracket1: Bracket1,
    parser: impl Parser<Input<'i>, Output, ContextError>,
    bracket2: Bracket2,
) -> impl Parser<Input<'i>, Output, ContextError>
where
    Bracket1: Parser<Input<'i>, I1, ContextError>,
    Bracket2: Parser<Input<'i>, I2, ContextError>,
{
    trace(
        "bracketed",
        delimited(
            // `whitespace(1)` because of https://www.shellcheck.net/wiki/SC1054
            // > { is only recognized as the start of a command group when it's a separate token
            (bracket1, alt((line_ending.void(), space(1).void()))),
            cut_err(parser),
            // TODO: maybe remove this whitespace
            preceded(space(0..), cut_err(bracket2)),
        ),
    )
}

fn subshell(i: &mut Input<'_>) -> PResult<ast::SubshellCommand, ContextError> {
    trace("subshell", bracketed("(", compound_list, ")"))
        .map(ast::SubshellCommand)
        .parse_next(i)
}

/// `sequential-sep := (';' line-trailing?) | line-trailing`
fn sequential_sep(i: &mut Input<'_>) -> PResult<()> {
    // echo [;] echo\n
    // echo[;]  # comment\n
    // echo[ #comment\n]
    // echo[  \n]
    trace("sequential_sep", alt((";".void(), line_trailing.void()))).parse_next(i)
}

mod for_clause {
    use crate::parser2::complete_command::insignificant;

    use super::*;

    pub(super) fn for_clause(i: &mut Input<'_>) -> PResult<ast::ForClauseCommand> {
        trace(
            "for_clause",
            preceded(
                ("for", space(1..)),
                (
                    // TODO: how to use cut_err correctly here
                    cut_err(word::word),
                    // for i[     \n
                    //   \n
                    //\n
                    //     in 1 2 3  ;  \n
                    // \n
                    //   ]do ...
                    delimited(insignificant, cut_err(in_range), insignificant),
                    do_group,
                )
                    .map(|(i, values, body)| ast::ForClauseCommand {
                        variable_name: i.into_owned(),
                        values,
                        body,
                    }),
            ),
        )
        .parse_next(i)
    }

    fn in_range(i: &mut Input<'_>) -> PResult<Option<Vec<ast::Word>>> {
        terminated(
            // N.B. range may be optional
            opt(preceded(
                ("in", space(1..)),
                repeat(0.., terminated(word::word, space(0..))),
            )),
            sequential_sep,
        )
        .map(|v| {
            v.map(|v: Vec<_>| {
                v.into_iter()
                    .map(|s| ast::Word::from(s.into_owned()))
                    .collect()
            })
        })
        .parse_next(i)
    }
}

/// `do-group := do compound-list done`
fn do_group(i: &mut Input<'_>) -> PResult<ast::DoGroupCommand> {
    bracketed("do", compound_list, "done")
        .map(ast::DoGroupCommand)
        .parse_next(i)
}

mod if_clause {

    use super::*;

    pub(super) fn if_clause(i: &mut Input<'_>) -> PResult<ast::IfClauseCommand> {
        trace(
            "if_clause",
            winnow::seq!(ast::IfClauseCommand{
                    _: ("if", space_after_word),
                    condition:  preceded(space(0..), cut_err(compound_list)),
                    _: cut_err(delimited(space(0..), "then", space_after_word)),
                    then: trim_whitespace(0.., cut_err(compound_list), 0..),
                    elses: else_part.map(|v| (!v.is_empty()).then_some(v)),
                    _: preceded(space(0..), cut_err("fi"))
            }),
        )
        .parse_next(i)
    }

    fn else_part(i: &mut Input<'_>) -> PResult<Vec<ast::ElseClause>> {
        trace(
            "else_part",
            (
                repeat(0.., preceded(space(0..), conditional_else_part)),
                opt(preceded(space(0..), unconditional_else_part)),
            )
                .map(|(mut cs, u): (Vec<_>, _)| {
                    if let Some(uncond) = u {
                        cs.push(uncond);
                    }
                    cs
                }),
        )
        .parse_next(i)
    }
    fn conditional_else_part(i: &mut Input<'_>) -> PResult<ast::ElseClause> {
        winnow::seq!(ast::ElseClause{
            _: ("elif", space_after_word),
            condition: preceded(space(0..), cut_err(compound_list)).map(|c| Some(c)),
            _: cut_err(delimited(space(0..), "then", space_after_word)),
            body: preceded(space(0..), cut_err(compound_list))
        })
        .parse_next(i)
    }
    fn unconditional_else_part(i: &mut Input<'_>) -> PResult<ast::ElseClause> {
        preceded(
            ("else", space_after_word),
            preceded(space(0..), cut_err(compound_list)),
        )
        .map(|body| ast::ElseClause {
            condition: None,
            body,
        })
        .parse_next(i)
    }
}

/// `while-or-until-clause := ('while' | 'until') compound_list do_group`
fn while_or_until_clause(i: &mut Input<'_>) -> PResult<ast::CompoundCommand> {
    #[derive(Clone, Copy)]
    enum T {
        Until,
        While,
    }
    trace(
        "while_until_clause",
        (
            terminated(
                alt(("while".value(T::While), "until".value(T::Until))),
                space_after_word,
            ),
            cut_err((trim_whitespace(0.., compound_list, 0..), do_group)),
        ),
    )
    .map(|(ty, (c, d))| {
        let c = ast::WhileOrUntilClauseCommand(c, d);
        match ty {
            T::While => ast::CompoundCommand::WhileClause(c),
            T::Until => ast::CompoundCommand::UntilClause(c),
        }
    })
    .parse_next(i)
}

pub mod case_clause {
    use std::borrow::Cow;

    use super::*;
    pub fn case_clause(i: &mut Input<'_>) -> PResult<ast::CaseClauseCommand> {
        trace(
            "case_clause",
            (
                // value
                delimited(
                    ("case", space(1..)),
                    cut_err(word::word),
                    (
                        cut_err(alt((line_trailing, space(1..)))),
                        repeat(0.., line_trailing).map(|()| ()),
                    ),
                ),
                // cases
                delimited(
                    (
                        cut_err((space(0..), "in", space_after_word)),
                        repeat(0.., line_trailing).map(|()| ()),
                    ),
                    (
                        repeat(
                            0..,
                            preceded(
                                (repeat(0.., line_trailing).map(|()| ()), space(0..)),
                                case_item(false),
                            ),
                        ),
                        preceded(space(0..), opt(case_item(true))),
                    ),
                    (space(0..), cut_err("esac")),
                ),
            ),
        )
        .map(|(w, (mut cases, last)): (_, (Vec<_>, _))| {
            if let Some(last_item) = last {
                cases.push(last_item);
            }
            ast::CaseClauseCommand {
                value: ast::Word::from(w.into_owned()),
                cases,
            }
        })
        .parse_next(i)
    }

    fn case_item<'i, 's>(
        is_last: bool,
    ) -> impl Parser<Input<'i>, ast::CaseItem, ContextError> + 's {
        move |i: &mut Input<'i>| {
            trace(
                "case_item",
                (
                    terminated(
                        delimited(opt("("), pattern, cut_err(")")),
                        opt(line_trailing),
                    ),
                    opt(compound_list),
                    move |i: &mut Input<'i>| {
                        let post_action_parser =
                            delimited(space(0..), case_item_post_action, opt(line_trailing));
                        if is_last {
                            opt(post_action_parser)
                                .map(|p| p.unwrap_or(ast::CaseItemPostAction::ExitCase))
                                .parse_next(i)
                        } else {
                            cut_err(post_action_parser).parse_next(i)
                        }
                    },
                ),
            )
            .map(|(p, c, post)| ast::CaseItem {
                patterns: p
                    .into_iter()
                    .map(|p| ast::Word::from(p.into_owned()))
                    .collect(),
                cmd: c,
                post_action: post,
            })
            .parse_next(i)
        }
    }
    fn case_item_post_action(i: &mut Input<'_>) -> PResult<ast::CaseItemPostAction> {
        alt((
            ";;&".value(ast::CaseItemPostAction::ContinueEvaluatingCases),
            ";;".value(ast::CaseItemPostAction::ExitCase),
            ";&".value(ast::CaseItemPostAction::UnconditionallyExecuteNextCaseItem),
        ))
        .parse_next(i)
    }

    // a |  b   | c  )
    fn pattern<'i>(i: &mut Input<'i>) -> PResult<Vec<Cow<'i, str>>> {
        separated(1.., trim_whitespace(0.., word::word, 0..), "|").parse_next(i)
    }
}

// TODO: ariphmetic
mod arithmetics {
    // TODO: $(( http://www.oilshell.org/blog/2016/11/18.html
    use std::str::Utf8Error;

    use winnow::combinator::separated_pair;

    use super::*;

    pub fn arithmetic_command<'i>(i: &mut Input<'i>) -> PResult<ast::ArithmeticCommand> {
        trace(
            "arithmetic_command",
            delimited("((", trim_whitespace(0.., arithmetic_expression, 0..), "))"),
        )
        .map(|expr| ast::ArithmeticCommand { expr })
        .parse_next(i)
    }
    fn arithmetic_expression<'i>(i: &mut Input<'i>) -> PResult<ast::UnexpandedArithmeticExpr> {
        repeat(0.., trim_whitespace(0.., arithmetic_expression_piece, 0..))
            .with_taken()
            .try_map(|((), s)| {
                Ok::<_, Utf8Error>(ast::UnexpandedArithmeticExpr {
                    value: std::str::from_utf8(s)?.to_string(),
                })
            })
            .parse_next(i)
    }
    fn arithmetic_expression_piece<'i>(i: &mut Input<'i>) -> PResult<()> {
        alt((
            trim_whitespace(
                0..,
                delimited(
                    "(",
                    repeat(0.., trim_whitespace(0.., arithmetic_expression_piece, 0..)),
                    ")",
                ),
                0..,
            ),
            arithmetic_end,
        ))
        .parse_next(i)
    }
    fn arithmetic_end<'i>(i: &mut Input<'i>) -> PResult<()> {
        // // TODO: evaluate arithmetic end; the semicolon is used in arithmetic for loops.
        alt(("))", ";")).void().parse_next(i)
    }

    // N.B. The arithmetic for loop is a non-sh extension.
    pub fn arithmetic_for_clause(i: &mut Input<'_>) -> PResult<ast::ArithmeticForClauseCommand> {
        winnow::seq!(ast::ArithmeticForClauseCommand{
            _: separated_pair("for", space(0..), "(("),
            initializer: terminated(trim_whitespace(0.., opt(arithmetic_expression), 0..), ";"),
            condition: terminated(trim_whitespace(0.., opt(arithmetic_expression), 0..), ";"),
            updater: trim_whitespace(0.., opt(arithmetic_expression), 0..),
            _: trim_whitespace(0.., "))", 0..),
            _: sequential_sep,
            body: do_group
        })
        .parse_next(i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use self::{case_clause::case_clause, for_clause::for_clause, if_clause::if_clause};
    use crate::parser2::tests::{expect_echo, test_variants_err, test_variants_ok};

    mod test_brace_group {

        use super::*;

        test_variants_ok! {brace_group(
            basic("{ echo hello;}"),
            new_lines("{

    echo hello

        }"),
            comments("{ #comment

    #comment 1
echo          hello    ;   # comment 2
# comment 3

}"),

        ) ->
            ast::BraceGroupCommand(ast::CompoundList(vec![expect_echo("hello")]))
        }

        test_variants_err! {brace_group(
            empty("{}"),
            without_space("{:;}"),
            without_semicolon("{:;}"),
            wrong_comment("{ # comment}"),
            newline_semicolon("{ :
        ;}"),
        )}
    }

    mod test_compound_list {
        use crate::parser2::tests::input;

        use super::*;

        #[test]
        fn for_loop_with_newline() -> crate::parser2::tests::Result<()> {
            use ast::*;
            let i = input("for i in 1; do echo hello; done\n");
            let expected = CompoundList(vec![CompoundListItem(
                AndOrList {
                    first: Pipeline {
                        bang: false,
                        seq: vec![Command::Compound(
                            CompoundCommand::ForClause(ForClauseCommand {
                                variable_name: "i".into(),
                                values: Some(vec![Word::new("1")]),
                                body: DoGroupCommand(CompoundList(vec![expect_echo("hello")])),
                            }),
                            None,
                        )],
                    },
                    additional: vec![],
                },
                SeparatorOperator::Sequence,
            )]);
            assert_eq!(compound_list.parse(i)?, expected);
            Ok(())
        }

        test_variants_ok! {compound_list(
            basic(r#"echo hello"#),
            basic_semicolon(r#"echo hello"#),
            basic_with_comment(r#"echo hello #comment"#),
            start_with_a_comment(
                r#"#comment
                # comment 2
                echo hello
                #comment3
                "#
            ),
        ) ->
            ast::CompoundList(vec![expect_echo("hello")])
        }
        test_variants_ok! {compound_list(
            list(r#"echo hello  ; echo world;"#),
            multiline_with_comments(
                r#"echo hello #comment
                # comment 2
                echo world
                # comment
                "#
            ),
        ) ->
            ast::CompoundList(vec![expect_echo("hello"), expect_echo("world")])
        }
    }

    mod test_while_until {
        use super::*;
        test_variants_ok! {while_or_until_clause(
            multiline_predicate(
                r#"while
                echo hello
                echo world
            do
                echo body
            done"#
            ),
            a_lot_of_comments(
                r#"while #comment1
        # comment 2
        echo hello
        echo world
        # comment 3
    do # comment 4
        echo body # comment 5
        # comment 6
    done"#
            )
        ) ->
            ast::CompoundCommand::WhileClause(ast::WhileOrUntilClauseCommand(ast::CompoundList(vec![
                expect_echo("hello"),
                expect_echo("world"),
            ]), ast::DoGroupCommand(ast::CompoundList(vec![ expect_echo("body") ]))))
        }
    }

    mod test_for_clause {
        use crate::parser2::tests::input;

        use super::*;

        fn expect_for_clause(values: Option<Vec<ast::Word>>) -> ast::ForClauseCommand {
            ast::ForClauseCommand {
                variable_name: "i".into(),
                values,
                body: ast::DoGroupCommand(ast::CompoundList(vec![expect_echo("hello")])),
            }
        }

        test_variants_ok! {for_clause(
            basic(
                r#"for i in 1 2 3; do
                echo hello
                done"#
            ),
            oneline(
                r#"for i in 1 2 3; do echo hello; done"#
            ),
            comments(
                r#"for i # comment
                # comment 2
                in 1 2 3 # comment 3
                # comment 4
                do
                echo hello
                done"#
            )
            ) -> expect_for_clause(Some(vec![ast::Word::new("1"), ast::Word::new("2"), ast::Word::new("3") ]))
        }
        test_variants_ok! {for_clause(
            without_values(
                r#"for i; do
                echo hello
                done"#
            ),
            without_values_with_comments(
                r#"for i # comment
                # comment 2
                do
                echo hello
                done"#
            ),
            ) -> expect_for_clause(None)
        }
        test_variants_ok! {for_clause(
            empty_values(
                r#"for i in ; do
                echo hello
                done"#
            ),
            )  -> expect_for_clause(Some(vec![]))
        }

        #[test]
        fn ambigiuos_done() -> crate::parser2::tests::Result<()> {
            let i = input(
                r#"for i; do
echo done

done"#,
            );
            let r = for_clause.parse(i)?;
            let expected = ast::ForClauseCommand {
                variable_name: "i".into(),
                values: None,
                body: ast::DoGroupCommand(ast::CompoundList(vec![expect_echo("done")])),
            };
            assert_eq!(r, expected);

            Ok(())
        }
    }

    mod test_if_else {
        use super::*;

        test_variants_ok! {if_clause(
        simple_oneliner(r#"if echo hello; then echo world; fi"#),
        multiline(
            r#"if
 echo hello

  then echo world

fi"#
        ),
        comments(
            r#"if echo hello # comment1
# comment2
 then # comment 3
    echo world ;
    # comment 4
      fi"#
        ),


            )  -> ast::IfClauseCommand{
                condition: ast::CompoundList(vec![expect_echo("hello")]),
                then: ast::CompoundList(vec![expect_echo("world")]),
                elses: None
            }
        }

        test_variants_ok! {if_clause(
        if_else(
            r#"if echo hello

 then
    echo world;

else echo elseworld
      fi"#
        ),

            )  -> ast::IfClauseCommand{
                condition: ast::CompoundList(vec![expect_echo("hello")]),
                then: ast::CompoundList(vec![expect_echo("world")]),
                elses: Some(vec![ast::ElseClause{
                    condition: None,
                    body: ast::CompoundList(vec![expect_echo("elseworld")])
                }])
            }
        }
        test_variants_ok! {if_clause(
        if_elif_else(
            r#"if echo hello

 then # comment
    echo world
    elif echo elif1
then
    echo elifthen1; else
    echo elseend
      fi"#
        ),
            )  -> ast::IfClauseCommand{
                condition: ast::CompoundList(vec![expect_echo("hello")]),
                then: ast::CompoundList(vec![expect_echo("world")]),
                elses: Some(vec![
                    ast::ElseClause{
                    condition: Some(ast::CompoundList(vec![expect_echo("elif1")])),
                    body: ast::CompoundList(vec![expect_echo("elifthen1")])
                },
                    ast::ElseClause{
                    condition: None,
                    body: ast::CompoundList(vec![expect_echo("elseend")])
                },
                ])
            }
        }
    }

    mod test_case_clause {
        use super::*;
        test_variants_ok! {case_clause(
        complex(
            r#"case "1" # comment 1

            in

            1);; # comment1

# comment 3
2 | "patt#ern2" | 2he@llo,he_llo) echo hello

;&
            (:)  echo world; ;;&
            esac"#
        ),
            )  -> ast::CaseClauseCommand{
                value: ast::Word::new("1"),
                cases: vec![
                    ast::CaseItem{
                        patterns: vec![ast::Word::new("1")],
                        cmd: None,
                        post_action: ast::CaseItemPostAction::ExitCase
                    },
                    ast::CaseItem{
                        patterns: vec![ast::Word::new("2"), ast::Word::new("patt#ern2"), ast::Word::new("2he@llo,he_llo")],
                        cmd: Some(ast::CompoundList(vec![expect_echo("hello")])),
                        post_action: ast::CaseItemPostAction::UnconditionallyExecuteNextCaseItem
                    },
                    ast::CaseItem{
                        patterns: vec![ast::Word::new(":")],
                        cmd: Some(ast::CompoundList(vec![expect_echo("world")])),
                        post_action: ast::CaseItemPostAction::ContinueEvaluatingCases
                    }
                ]

            }
        }
    }
}
