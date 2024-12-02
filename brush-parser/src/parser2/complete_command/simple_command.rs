use std::borrow::Cow;

use winnow::{
    combinator::{
        alt, cut_err, delimited, eof, fail, not, opt, preceded, repeat, separated, trace,
    },
    dispatch,
    token::{any, one_of},
    PResult, Parser,
};

use crate::{
    ast,
    parser2::{
        assignment,
        custom_combinators::{expand_later, non_posix_extension},
        io::{self, process_substitution},
        trivia::space,
        word::{self},
        Input,
    },
};

/// `simple-command := (prefix (name suffix?)?) | (name suffix?)`
pub fn simple_command(i: &mut Input<'_>) -> PResult<ast::SimpleCommand> {
    trace("simple_command", move |i: &mut Input<'_>| {
        let prefix = opt(cmd_prefix).parse_next(i)?;
        let (cmd_name, suffix) = if prefix.is_some() {
            // if there is a prefix the rest of the command are optional
            opt((
                // N.B should be a whitespace between cmd_prefix and cmd_name
                delimited(space(1..), cmd_name.map(Some), space(1..)),
                opt(cmd_suffix),
            ))
            .map(|o| o.unwrap_or((None, None)))
            .parse_next(i)?
        } else {
            (cmd_name.map(Some), opt(preceded(space(1..), cmd_suffix))).parse_next(i)?
        };
        Ok(ast::SimpleCommand {
            prefix,
            word_or_name: cmd_name.map(|n| ast::Word::from(n.into_owned())),
            suffix,
        })
    })
    .parse_next(i)
}

pub fn cmd_prefix(i: &mut Input<'_>) -> PResult<ast::CommandPrefix> {
    trace(
        "cmd_prefix",
        separated(
            1..,
            alt((
                io::io_redirect.map(|i| ast::CommandPrefixOrSuffixItem::IoRedirect(i)),
                assignment::assignment.map(|(assignment, word)| {
                    ast::CommandPrefixOrSuffixItem::AssignmentWord(assignment, word)
                }),
            )),
            space(1..),
        ),
    )
    .map(ast::CommandPrefix)
    .parse_next(i)
}

// TODO: check rules
// cmd_name         : WORD                   /* Apply rule 7a */
//                  ;
// cmd_word         : WORD                   /* Apply rule 7b */
fn cmd_name<'i>(i: &mut Input<'i>) -> PResult<Cow<'i, str>> {
    trace(
        "cmd_name",
        alt((
            // N.B maybe subshell $() or `` that we will expand later inside the interpreter
            expand_later.map(|s| Cow::Borrowed(s)),
            // Disallow empty names.
            // This is differs from Bash. But according to:
            // https://unix.stackexchange.com/questions/66965/files-with-empty-names
            // filenames cannot be empty. So it is a nice user experience enchantment.
            word::non_reserved(word::non_empty(word::word)),
        )),
    )
    .parse_next(i)
}

pub fn cmd_suffix(i: &mut Input<'_>) -> PResult<ast::CommandSuffix> {
    trace(
        "cmd_suffix",
        repeat(
            1..,
            delimited(
                // N.B backtrack optimization
                // TODO: use constants
                (not(one_of(('#', ';', '&', '|', '\n', '\r'))), not(eof)),
                alt((
                    io::io_redirect.map(|i| ast::CommandPrefixOrSuffixItem::IoRedirect(i)),
                    assignment::assignment.map(|(assignment, word)| {
                        ast::CommandPrefixOrSuffixItem::AssignmentWord(assignment, word)
                    }),
                    word::word.map(|w| {
                        ast::CommandPrefixOrSuffixItem::Word(ast::Word::from(w.into_owned()))
                    }),
                    non_posix_extension(process_substitution).map(|(kind, subshell)| {
                        ast::CommandPrefixOrSuffixItem::ProcessSubstitution(kind, subshell)
                    }),
                )),
                // a newline maybe escaped
                // echo    \
                //    hello
                (space(0..), opt((b"\\\n", space(0..)))),
            ),
        ),
    )
    .map(ast::CommandSuffix)
    .parse_next(i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser2::tests::test_variants_ok;
    use ast::*;

    test_variants_ok! {simple_command(
    basic(
        r#"echo hello "wor ld" hel-lo world"#
    ),
    escapes(
        r#""echo"    hello \
        wor\ ld \
        hel-lo\
        world"#
    ),
        )  -> SimpleCommand{
            prefix: None,
            word_or_name: Some(Word::new("echo")),
            suffix: Some(CommandSuffix(vec![CommandPrefixOrSuffixItem::Word(Word::new("hello")),
            CommandPrefixOrSuffixItem::Word(Word::new("wor ld")),
            CommandPrefixOrSuffixItem::Word(Word::new("hel-lo")),
            CommandPrefixOrSuffixItem::Word(Word::new("world"))
            ]))
        }
    }
    test_variants_ok! {simple_command(
    assignment(
        r#"FOO=1 <filename echo hello "wor ld" hel-lo world  2>&1"#
    )
        )  -> SimpleCommand{
            prefix: Some(CommandPrefix(vec!{
                CommandPrefixOrSuffixItem::AssignmentWord(
                    Assignment { name: AssignmentName::VariableName("FOO".into()), value: AssignmentValue::Scalar(Word::new("1")), append: false },
                    Word::new("FOO=1")
                ),
                CommandPrefixOrSuffixItem::IoRedirect(IoRedirect::File(None,
                IoFileRedirectKind::Write,
                IoFileRedirectTarget::Filename(Word::new("filename"))))
            })),
            word_or_name: Some(Word::new("echo")),
            suffix: Some(CommandSuffix(vec![CommandPrefixOrSuffixItem::Word(Word::new("hello")),
            CommandPrefixOrSuffixItem::Word(Word::new("wor ld")),
            CommandPrefixOrSuffixItem::Word(Word::new("hel-lo")),
            CommandPrefixOrSuffixItem::Word(Word::new("world")),
            CommandPrefixOrSuffixItem::IoRedirect(IoRedirect::File(Some(2), IoFileRedirectKind::DuplicateOutput, IoFileRedirectTarget::Fd(1)))
            ]))
        }
    }
}
