use crate::{ast, ParserOptions};

mod assignment;
mod complete_command;
mod custom_combinators;
mod expansion;
mod io;
mod pipeline;
mod precedence;
mod trivia;
mod word;

use winnow::{
    combinator::{cut_err, eof, opt, preceded, repeat_till, terminated, trace},
    PResult, Parser,
};

use self::complete_command::insignificant;

type Input<'b> = winnow::Stateful<winnow::Located<&'b winnow::BStr>, ParserOptions>;

/// Top level function to start parsing a script
pub fn parse_program(
    state: ParserOptions,
    input: &str,
) -> Result<ast::Program, crate::error::ParseError> {
    let parse_result = program.parse(new_input(state, input));

    match parse_result {
        Ok(program) => {
            tracing::debug!(target: "parse", "PROG: {:?}", program);
            Ok(program)
        }
        Err(parse_error) => {
            tracing::debug!(target: "parse", "Parse error: {:?}", parse_error);
            // TODO: errors
            Err(crate::error::ParseError::ParsingAtEndOfInput)
        }
    }
}

#[cached::proc_macro::cached(size = 64, result = true)]
pub fn cacheable_parse_program(
    state: ParserOptions,
    input: String,
) -> Result<ast::Program, crate::error::ParseError> {
    parse_program(state, input.as_str())
}

pub(crate) fn new_input(options: ParserOptions, s: &str) -> Input<'_> {
    winnow::Stateful {
        input: winnow::Located::new(winnow::BStr::new(s)),
        state: options,
    }
}

/// `program := bom? insignificant_lines* complete_command* insignificant_lines* eof`
pub(crate) fn program(i: &mut Input<'_>) -> PResult<ast::Program> {
    trace(
        "program",
        // TODO: streaming
        preceded(
            // Remove BOM if present
            (trace("BOM", opt(b"\xEF\xBB\xBF")), insignificant),
            repeat_till(
                0..,
                terminated(
                    cut_err(complete_command::complete_command.map(ast::CompoundList)),
                    insignificant,
                ),
                eof.void(),
            ),
        )
        .map(|(complete_commands, ())| ast::Program { complete_commands }),
    )
    .parse_next(i)
}

#[cfg(test)]
mod tests {
    // TODO: tests https://github.com/andrewhickman/protox/blob/main/protox-parse/src/parse/tests.rs
    use super::*;

    pub(crate) type Result<T> = std::result::Result<
        T,
        winnow::error::ParseError<Input<'static>, winnow::error::ContextError>,
    >;

    pub(crate) fn input<'i>(i: &'i str) -> Input<'i> {
        crate::parser2::new_input(crate::ParserOptions::default(), i)
    }

    macro_rules! test_variants_ok {
        ($parser:ident($($case:ident($i:literal)),+ $(,)?) -> $expected:expr) => {
            $(
                #[test]
                fn $case() -> std::result::Result<(), winnow::error::ParseError<Input<'static>, winnow::error::ContextError>> {
                    assert_eq!($parser.parse(crate::parser2::new_input(crate::ParserOptions::default(), ($i)))?, $expected);
                    Ok(())
                }
            )+

        };
    }
    pub(crate) use test_variants_ok;

    macro_rules! test_variants_err {
        ($parser:ident($($case:ident($i:literal)),+ $(,)?)) => {
            $(
                #[test]
                fn $case() {
                   assert_matches::assert_matches!($parser.parse(crate::parser2::new_input(crate::ParserOptions::default(), $i)), Err(_));
                }
            )+

        };
    }
    pub(crate) use test_variants_err;

    pub(crate) fn expect_echo(word: &str) -> ast::CompoundListItem {
        ast::CompoundListItem(
            ast::AndOrList {
                first: ast::Pipeline {
                    bang: false,
                    seq: vec![ast::Command::Simple(ast::SimpleCommand {
                        prefix: None,
                        word_or_name: Some(ast::Word::new("echo")),
                        suffix: Some(ast::CommandSuffix(vec![
                            ast::CommandPrefixOrSuffixItem::Word(ast::Word::new(word)),
                        ])),
                    })],
                },
                additional: vec![],
            },
            ast::SeparatorOperator::Sequence,
        )
    }

    use ast::*;

    #[test]
    fn empty() -> Result<()> {
        assert_eq!(
            program.parse(input(""))?,
            Program {
                complete_commands: vec![]
            }
        );
        Ok(())
    }

    #[test]
    fn basic() -> Result<()> {
        let i = input(
            r#"

#!/usr/bin/env bash

for f in A B C; do

    # sdfsdf
    echo "${f@L}" >&2

   done

"#,
        );
        let expected = ast::Program {
            complete_commands: vec![CompoundList(vec![CompoundListItem(
                AndOrList {
                    first: Pipeline {
                        bang: false,
                        seq: vec![Command::Compound(
                            CompoundCommand::ForClause(ForClauseCommand {
                                variable_name: "f".into(),
                                values: Some(vec![Word::new("A"), Word::new("B"), Word::new("C")]),
                                body: DoGroupCommand(CompoundList(vec![CompoundListItem(
                                    AndOrList {
                                        first: Pipeline {
                                            bang: false,
                                            seq: vec![Command::Simple(SimpleCommand {
                                                prefix: None,
                                                word_or_name: Some(Word::new("echo")),
                                                suffix: Some(CommandSuffix(vec![
                                                    CommandPrefixOrSuffixItem::Word(Word::new(
                                                        "${f@L}",
                                                    )),
                                                    CommandPrefixOrSuffixItem::IoRedirect(
                                                        IoRedirect::File(
                                                            None,
                                                            IoFileRedirectKind::DuplicateOutput,
                                                            IoFileRedirectTarget::Fd(2),
                                                        ),
                                                    ),
                                                ])),
                                            })],
                                        },
                                        additional: vec![],
                                    },
                                    SeparatorOperator::Sequence,
                                )])),
                            }),
                            None,
                        )],
                    },
                    additional: vec![],
                },
                SeparatorOperator::Sequence,
            )])],
        };

        let r = program.parse(i)?;
        assert_eq!(r, expected);
        Ok(())
    }

    #[test]
    fn two_complete_commands() -> Result<()> {
        let i = input(
            "
    echo hello # comment
    # comment 2
    echo world;
    # comment3
    ",
        );
        let expected = Program {
            complete_commands: vec![
                CompoundList(vec![expect_echo("hello")]),
                CompoundList(vec![expect_echo("world")]),
            ],
        };
        assert_eq!(program.parse(i)?, expected);
        Ok(())
    }

    #[test]
    fn ambigiuos_for() -> crate::parser2::tests::Result<()> {
        let i = input(r#"for for in for; do for=for; done; echo $for"#);
        let expected =
            ast::Program {
                complete_commands: vec![CompoundList(vec![
                    CompoundListItem(
                        AndOrList {
                            first: Pipeline {
                                bang: false,
                                seq: vec![Command::Compound(
                                    CompoundCommand::ForClause(ForClauseCommand {
                                        variable_name: "for".into(),
                                        values: Some(vec![Word::new("for")]),
                                        body: DoGroupCommand(CompoundList(vec![CompoundListItem(
                                            AndOrList {
                                                first: Pipeline {
                                                    bang: false,
                                                    seq: vec![Command::Simple(SimpleCommand {
                        prefix: Some(CommandPrefix(vec![CommandPrefixOrSuffixItem::AssignmentWord(
                            Assignment {
                                name: AssignmentName::VariableName("for".into()),
                                value: AssignmentValue::Scalar(Word::new("for")),
                                append: false,
                            },
                            Word::new("for=for"),
                        )])),
                        word_or_name: None,
                        suffix: None,
                    })],
                                                },
                                                additional: vec![],
                                            },
                                            SeparatorOperator::Sequence,
                                        )])),
                                    }),
                                    None,
                                )],
                            },
                            additional: vec![],
                        },
                        SeparatorOperator::Sequence,
                    ),
                    expect_echo("$for"),
                ])],
            };

        let r = program.parse(i)?;
        assert_eq!(r, expected);

        Ok(())
    }

    // KCORE=$(($SUDO "$PERF" buildid-cache -v -f -k /proc/kcore >/dev/null) 2>&1)
    // echo $((time -p $* >/dev/null) 2>&1) | awk '{print $4 "u " $6 "s " $2 "r"}'
    // hg_relative_sourcedir=$((cd $sourcedir; pwd) | sed -e "s|$(hg root)/||")
}
