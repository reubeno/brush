use anyhow::Result;
use log::debug;

use crate::ast::{self, SeparatorOperator};
use crate::tokenizer::{Token, TokenEndReason, Tokenizer, Tokens};

pub enum ParseResult {
    Program(ast::Program),
    ParseError(Option<Token>),
    TokenizerError(String),
}

pub struct Parser<R> {
    reader: R,
}

impl<R: std::io::BufRead> Parser<R> {
    pub fn new(reader: R) -> Self {
        Parser { reader }
    }

    pub fn parse(&mut self, stop_on_unescaped_newline: bool) -> Result<ParseResult> {
        //
        // References:
        //   * https://www.gnu.org/software/bash/manual/bash.html#Shell-Syntax
        //   * https://mywiki.wooledge.org/BashParser
        //   * https://aosabook.org/en/v1/bash.html
        //   * https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html
        //

        let mut tokenizer = Tokenizer::new(&mut self.reader);

        let mut tokens = vec![];
        loop {
            let result = match tokenizer.next_token() {
                Ok(result) => result,
                Err(e) => {
                    return Ok(ParseResult::TokenizerError(e.to_string()));
                }
            };

            if let Some(token) = result.token {
                if log::log_enabled!(log::Level::Debug) {
                    debug!("TOKEN {}: {:?}", tokens.len(), token);
                }

                tokens.push(token);
            }

            if result.reason == TokenEndReason::EndOfInput {
                break;
            }

            if stop_on_unescaped_newline && result.reason == TokenEndReason::UnescapedNewLine {
                break;
            }
        }

        //
        // Apply aliases.
        //
        // TODO: implement aliasing
        //

        let tokens = Tokens { tokens };
        let parse_result = token_parser::program(&tokens);

        let result = match parse_result {
            Ok(program) => ParseResult::Program(program),
            Err(parse_error) => {
                debug!("Parse error: {:?}", parse_error);

                let approx_token_index = parse_error.location;

                let token_near_error;
                if approx_token_index < tokens.tokens.len() {
                    token_near_error = Some(tokens.tokens[approx_token_index].clone());
                } else {
                    token_near_error = None;
                }

                ParseResult::ParseError(token_near_error)
            }
        };

        if log::log_enabled!(log::Level::Debug) {
            if let ParseResult::Program(program) = &result {
                debug!("PROG: {:#?}", program);
            }
        }

        Ok(result)
    }
}

impl peg::Parse for Tokens {
    type PositionRepr = usize;

    fn start<'input>(&'input self) -> usize {
        0
    }

    fn is_eof<'input>(&'input self, p: usize) -> bool {
        p >= self.tokens.len()
    }

    fn position_repr<'input>(&'input self, p: usize) -> Self::PositionRepr {
        p
    }
}

impl<'a> peg::ParseElem<'a> for Tokens {
    type Element = &'a Token;

    fn parse_elem(&'a self, pos: usize) -> peg::RuleResult<Self::Element> {
        match self.tokens.get(pos) {
            Some(c) => peg::RuleResult::Matched(pos + 1, c),
            None => peg::RuleResult::Failed,
        }
    }
}

peg::parser! {
    grammar token_parser() for Tokens {
        pub(crate) rule program() -> ast::Program =
            linebreak() c:complete_commands() linebreak() { ast::Program { complete_commands: c } } /
            linebreak() { ast::Program { complete_commands: vec![] } }

        rule complete_commands() -> Vec<ast::CompleteCommand> =
            c:complete_command() ++ newline_list()

        rule complete_command() -> ast::CompleteCommand =
            all_but_last:(a:and_or() s:separator_op() { (a, s) })* last:and_or() last_sep:separator_op()? {
                let mut items = vec![];
                for item in all_but_last.into_iter() {
                    items.push(item);
                }

                // N.B. We default to synchronous if no separator op is given.
                items.push((last, last_sep.unwrap_or(SeparatorOperator::Sequence)));

                items
            }

        rule and_or() -> ast::AndOrList =
            first:pipeline() additional:_and_or_item()* { ast::AndOrList { first, additional } }

        rule _and_or_item() -> ast::AndOr =
            specific_operator("&&") linebreak() p:pipeline() { ast::AndOr::And(p) } /
            specific_operator("||") linebreak() p:pipeline() { ast::AndOr::Or(p) }


        rule pipeline() -> ast::Pipeline =
            bang:bang()? seq:pipe_sequence() { ast::Pipeline { bang: bang.is_some(), seq } }
        rule bang() -> bool = specific_word("!") { true }

        rule pipe_sequence() -> Vec<ast::Command> =
            c:command() ++ (specific_operator("|") linebreak()) { c }

        // TODO: Figure out why we needed to move the function definition branch up
        rule command() -> ast::Command =
            f:function_definition() { ast::Command::Function(f) } /
            c:simple_command() { ast::Command::Simple(c) } /
            c:compound_command() r:redirect_list()? { ast::Command::Compound(c, r) } /
            // N.B. Extended test commands are bash extensions.
            c:extended_test_command() { ast::Command::ExtendedTest(c) } /
            expected!("command")

        rule compound_command() -> ast::CompoundCommand =
            b:brace_group() { ast::CompoundCommand::BraceGroup(b) } /
            s:subshell() { ast::CompoundCommand::Subshell(s) } /
            f:for_clause() { ast::CompoundCommand::ForClause(f) } /
            c:case_clause() { ast::CompoundCommand::CaseClause(c) } /
            i:if_clause() { ast::CompoundCommand::IfClause(i) } /
            w:while_clause() { ast::CompoundCommand::WhileClause(w) } /
            u:until_clause() { ast::CompoundCommand::UntilClause(u) } /
            expected!("compound command")

        rule subshell() -> ast::SubshellCommand =
            specific_operator("(") c:compound_list() specific_operator(")") { c }

        rule compound_list() -> ast::CompoundList =
            linebreak() first:and_or() remainder:(s:separator() l:and_or() { (s, l) })* last_sep:separator()? {
                let mut and_ors = vec![first];
                let mut seps = vec![];

                for (sep, ao) in remainder.into_iter() {
                    seps.push(sep.unwrap_or(SeparatorOperator::Sequence));
                    and_ors.push(ao);
                }

                // N.B. We default to synchronous if no separator op is given.
                let last_sep = last_sep.unwrap_or(None);
                seps.push(last_sep.unwrap_or(SeparatorOperator::Sequence));

                let mut items = vec![];
                for (i, ao) in and_ors.into_iter().enumerate() {
                    items.push((ao, seps[i].clone()));
                }

                items
            }

        rule for_clause() -> ast::ForClauseCommand =
            specific_word("for") n:name() linebreak() _in() w:wordlist()? sequential_sep() d:do_group() {
                ast::ForClauseCommand { variable_name: n.to_owned(), values: w, body: d }
            } /
            specific_word("for") n:name() sequential_sep()? d:do_group() {
                ast::ForClauseCommand { variable_name: n.to_owned(), values: None, body: d }
            }

        rule extended_test_command() -> ast::ExtendedTestExpression =
            specific_word("[[") e:extended_test_expression() specific_word("]]") { e }

        // TODO: implement test expressions
        rule extended_test_expression() -> ast::ExtendedTestExpression =
            specific_word("-a") f:word() { ast::ExtendedTestExpression::FileExists(f.to_owned()) } /
            specific_word("-b") f:word() { ast::ExtendedTestExpression::FileExistsAndIsBlockSpecialFile(f.to_owned()) } /
            specific_word("-c") f:word() { ast::ExtendedTestExpression::FileExistsAndIsCharSpecialFile(f.to_owned()) } /
            specific_word("-d") f:word() { ast::ExtendedTestExpression::FileExistsAndIsDir(f.to_owned()) } /
            specific_word("-e") f:word() { ast::ExtendedTestExpression::FileExists(f.to_owned()) } /
            specific_word("-f") f:word() { ast::ExtendedTestExpression::FileExistsAndIsRegularFile(f.to_owned()) } /
            specific_word("-g") f:word() { ast::ExtendedTestExpression::FileExistsAndIsSetgid(f.to_owned()) } /
            specific_word("-h") f:word() { ast::ExtendedTestExpression::FileExistsAndIsSymlink(f.to_owned()) } /
            specific_word("-k") f:word() { ast::ExtendedTestExpression::FileExistsAndHasStickyBit(f.to_owned()) } /
            specific_word("-n") f:word() { ast::ExtendedTestExpression::StringHasNonZeroLength(f.to_owned()) } /
            specific_word("-o") f:word() { ast::ExtendedTestExpression::ShellOptionEnabled(f.to_owned()) } /
            specific_word("-p") f:word() { ast::ExtendedTestExpression::FileExistsAndIsFifo(f.to_owned()) } /
            specific_word("-r") f:word() { ast::ExtendedTestExpression::FileExistsAndIsReadable(f.to_owned()) } /
            specific_word("-s") f:word() { ast::ExtendedTestExpression::FileExistsAndIsNotZeroLength(f.to_owned()) } /
            specific_word("-t") f:word() { ast::ExtendedTestExpression::FdIsOpenTerminal(f.to_owned()) } /
            specific_word("-u") f:word() { ast::ExtendedTestExpression::FileExistsAndIsSetuid(f.to_owned()) } /
            specific_word("-v") f:word() { ast::ExtendedTestExpression::ShellVariableIsSetAndAssigned(f.to_owned()) } /
            specific_word("-w") f:word() { ast::ExtendedTestExpression::FileExistsAndIsWritable(f.to_owned()) } /
            specific_word("-x") f:word() { ast::ExtendedTestExpression::FileExistsAndIsExecutable(f.to_owned()) } /
            specific_word("-z") f:word() { ast::ExtendedTestExpression::StringHasZeroLength(f.to_owned()) } /
            specific_word("-G") f:word() { ast::ExtendedTestExpression::FileExistsAndOwnedByEffectiveGroupId(f.to_owned()) } /
            specific_word("-L") f:word() { ast::ExtendedTestExpression::FileExistsAndIsSymlink(f.to_owned()) } /
            specific_word("-N") f:word() { ast::ExtendedTestExpression::FileExistsAndModifiedSinceLastRead(f.to_owned()) } /
            specific_word("-O") f:word() { ast::ExtendedTestExpression::FileExistsAndOwnedByEffectiveUserId(f.to_owned()) } /
            specific_word("-R") f:word() { ast::ExtendedTestExpression::ShellVariableIsSetAndNameRef(f.to_owned()) } /
            specific_word("-S") f:word() { ast::ExtendedTestExpression::FileExistsAndIsSocket(f.to_owned()) } /
            left:word() specific_word("-ef") right:word() { ast::ExtendedTestExpression::FilesReferToSameDeviceAndInodeNumbers(left.to_owned(), right.to_owned()) } /
            left:word() specific_word("-eq") right:word() { ast::ExtendedTestExpression::ArithmeticEqualTo(left.to_owned(), right.to_owned()) } /
            left:word() specific_word("-ge") right:word() { ast::ExtendedTestExpression::ArithmeticGreaterThanOrEqualTo(left.to_owned(), right.to_owned()) } /
            left:word() specific_word("-gt") right:word() { ast::ExtendedTestExpression::ArithmeticGreaterThan(left.to_owned(), right.to_owned()) } /
            left:word() specific_word("-le") right:word() { ast::ExtendedTestExpression::ArithmeticLessThanOrEqualTo(left.to_owned(), right.to_owned()) } /
            left:word() specific_word("-lt") right:word() { ast::ExtendedTestExpression::ArithmeticLessThan(left.to_owned(), right.to_owned()) } /
            left:word() specific_word("-ne") right:word() { ast::ExtendedTestExpression::ArithmeticNotEqualTo(left.to_owned(), right.to_owned()) } /
            left:word() specific_word("-nt") right:word() { ast::ExtendedTestExpression::LeftFileIsNewerOrExistsWhenRightDoesNot(left.to_owned(), right.to_owned()) } /
            left:word() specific_word("-ot") right:word() { ast::ExtendedTestExpression::LeftFileIsOlderOrDoesNotExistWhenRightDoes(left.to_owned(), right.to_owned()) } /
            left:word() specific_word("==") right:word() { ast::ExtendedTestExpression::StringsAreEqual(left.to_owned(), right.to_owned()) } /
            left:word() specific_word("=") right:word() { ast::ExtendedTestExpression::StringsAreEqual(left.to_owned(), right.to_owned()) } /
            left:word() specific_word("!=") right:word() { ast::ExtendedTestExpression::StringsNotEqual(left.to_owned(), right.to_owned()) } /
            left:word() specific_word("<") right:word() { ast::ExtendedTestExpression::LeftSortsBeforeRight(left.to_owned(), right.to_owned()) } /
            left:word() specific_word(">") right:word() { ast::ExtendedTestExpression::LeftSortsAfterRight(left.to_owned(), right.to_owned()) } /
            [Token::Word(w, _)] { ast::ExtendedTestExpression::StringHasNonZeroLength(w.to_owned()) }

        rule name() -> &'input str =
            [Token::Word(w, _)] { w }

        rule _in() -> () =
            specific_word("in") { () }

        rule wordlist() -> Vec<String> =
            (w:word() { w.to_owned() })+

        pub(crate) rule case_clause() -> ast::CaseClauseCommand =
            specific_word("case") w:word() linebreak() _in() linebreak() c:case_list() specific_word("esac") {
                ast::CaseClauseCommand { value: w.to_owned(), cases: c }
            } /
            specific_word("case") w:word() linebreak() _in() linebreak() c:case_list_ns() specific_word("esac") {
                ast::CaseClauseCommand { value: w.to_owned(), cases: c }
            } /
            specific_word("case") w:word() linebreak() _in() linebreak() specific_word("esac") {
                ast::CaseClauseCommand{ value: w.to_owned(), cases: vec![] }
            }

        rule case_list_ns() -> Vec<ast::CaseItem> =
            first:case_list()? last:case_item_ns() {
                let mut items = vec![];
                if let Some(mut first) = first {
                    for item in first.into_iter() {
                        items.push(item);
                    }
                }
                items.push(last);
                items
            }

        rule case_list() -> Vec<ast::CaseItem> =
            c:case_item()+

        pub(crate) rule case_item_ns() -> ast::CaseItem =
            specific_operator("(")? p:pattern() specific_operator(")") c:compound_list() {
                ast::CaseItem { patterns: p, cmd: Some(c) }
            } /
            specific_operator("(")? p:pattern() specific_operator(")") linebreak() {
                ast::CaseItem { patterns: p, cmd: None }
            }

        pub(crate) rule case_item() -> ast::CaseItem =
            specific_operator("(")? p:pattern() specific_operator(")") linebreak() specific_operator(";;") linebreak() {
                ast::CaseItem { patterns: p, cmd: None }
            } /
            specific_operator("(")? p:pattern() specific_operator(")") c:compound_list() specific_operator(";;") linebreak() {
                ast::CaseItem { patterns: p, cmd: Some(c) }
            }

        rule pattern() -> Vec<String> =
            (w:word() { w.to_owned() }) ++ specific_operator("|")

        rule if_clause() -> ast::IfClauseCommand =
            specific_word("if") condition:compound_list() specific_word("then") then:compound_list() elses:else_part()? specific_word("fi") {
                ast::IfClauseCommand {
                    condition,
                    then,
                    elses,
                }
            }

        rule else_part() -> Vec<ast::ElseClause> =
            cs:_conditional_else_part()+ u:_unconditional_else_part()? {
                let mut parts = vec![];
                for c in cs.into_iter() {
                    parts.push(c);
                }

                if let Some(uncond) = u {
                    parts.push(uncond);
                }

                parts
            } /
            e:_unconditional_else_part() { vec![e] }

        rule _conditional_else_part() -> ast::ElseClause =
            specific_word("elif") condition:compound_list() specific_word("then") body:compound_list() {
                ast::ElseClause { condition: Some(condition), body }
            }

        rule _unconditional_else_part() -> ast::ElseClause =
            specific_word("else") body:compound_list() {
                ast::ElseClause { condition: None, body }
             }

        rule while_clause() -> ast::WhileClauseCommand =
            specific_word("while") c:compound_list() d:do_group() { (c, d) }

        rule until_clause() -> ast::UntilClauseCommand =
            specific_word("until") c:compound_list() d:do_group() { (c, d) }

        // N.B. bash allows use of the 'function' word to indicate a function definition.
        // TODO: Validate usage of this keyword.
        rule function_definition() -> ast::FunctionDefinition =
            fname:fname() specific_operator("(") specific_operator(")") linebreak() body:function_body() {
                ast::FunctionDefinition { fname: fname.to_owned(), body }
            } /
            specific_word("function") fname:fname() linebreak() body:function_body() {
                ast::FunctionDefinition { fname: fname.to_owned(), body }
            } /
            expected!("function definition")

        rule function_body() -> ast::FunctionBody =
            c:compound_command() r:redirect_list()? { (c, r) }

        rule fname() -> &'input str =
            name()

        rule brace_group() -> ast::BraceGroupCommand =
            specific_word("{") c:compound_list() specific_word("}") { c }

        rule do_group() -> ast::DoGroupCommand =
            specific_word("do") c:compound_list() specific_word("done") { c }

        rule simple_command() -> ast::SimpleCommand =
            prefix:cmd_prefix() word_or_name:cmd_word() suffix:cmd_suffix()? { ast::SimpleCommand { prefix: Some(prefix), word_or_name: Some(word_or_name.to_owned()), suffix } } /
            prefix:cmd_prefix() { ast::SimpleCommand { prefix: Some(prefix), word_or_name: None, suffix: None } } /
            word_or_name:cmd_name() suffix:cmd_suffix()? { ast::SimpleCommand { prefix: None, word_or_name: Some(word_or_name.to_owned()), suffix } } /
            expected!("simple command")

        rule cmd_name() -> &'input str =
            word()

        rule cmd_word() -> &'input str =
            !assignment_word() w:word() { w }

        rule cmd_prefix() -> ast::CommandPrefix =
            (i:io_redirect() { ast::CommandPrefixOrSuffixItem::IoRedirect(i) } /
                w:assignment_word() { ast::CommandPrefixOrSuffixItem::AssignmentWord(w) })+

        rule cmd_suffix() -> ast::CommandSuffix =
            (i:io_redirect() { ast::CommandPrefixOrSuffixItem::IoRedirect(i) } /
                w:word() { ast::CommandPrefixOrSuffixItem::Word(w.to_owned()) })+

        rule redirect_list() -> ast::RedirectList =
            io_redirect()+ /
            expected!("redirect list")

        rule io_redirect() -> ast::IoRedirect =
            n:io_number()? f:io_file() {
                let (kind, target) = f;
                ast::IoRedirect::File(n, kind, target)
            } /
            n:io_number()? h:io_here() { ast::IoRedirect::Here(n, h) } /
            expected!("I/O redirect")

        rule io_file() -> (ast::IoFileRedirectKind, ast::IoFileRedirectTarget) =
            specific_operator("<")  f:io_filename() { (ast::IoFileRedirectKind::Read, f) } /
            specific_operator("<&") f:io_filename_or_fd() { (ast::IoFileRedirectKind::DuplicateInput, f) } /
            specific_operator(">")  f:io_filename() { (ast::IoFileRedirectKind::Write, f) } /
            specific_operator(">&") f:io_filename_or_fd() { (ast::IoFileRedirectKind::DuplicateOutput, f) } /
            specific_operator(">>") f:io_filename() { (ast::IoFileRedirectKind::Append, f) } /
            specific_operator("<>") f:io_filename() { (ast::IoFileRedirectKind::ReadAndWrite, f) } /
            specific_operator(">|") f:io_filename() { (ast::IoFileRedirectKind::Clobber, f) }

        rule io_filename_or_fd() -> ast::IoFileRedirectTarget =
            fd:io_fd() { ast::IoFileRedirectTarget::Fd(fd) } /
            io_filename()

        rule io_fd() -> u32 =
            [Token::Word(w, _)] {? w.as_str().parse().or(Err("io_fd u32")) }

        rule io_filename() -> ast::IoFileRedirectTarget =
            f:filename() { ast::IoFileRedirectTarget::Filename(f.to_owned()) }

        rule filename() -> &'input str =
            word()

        rule io_here() -> ast::IoHere =
            specific_operator("<<") here_end:here_end() newline() doc:[_] { ast::IoHere { remove_tabs: false, here_end: here_end.to_owned(), doc: doc.to_str().to_owned() } } /
            specific_operator("<<-") here_end:here_end() newline() doc:[_] { ast::IoHere { remove_tabs: true, here_end: here_end.to_owned(), doc: doc.to_str().to_owned() } }

        rule here_end() -> &'input str =
            word()

        rule newline_list() -> () =
            newline()* { () }

        // N.B. We don't need to add a '?' to the invocation of the newline_list()
        // rule because it already allows 0 newlines.
        rule linebreak() -> () =
            quiet! {
                newline_list() { () }
            }

        rule separator_op() -> ast::SeparatorOperator =
            specific_operator("&") { ast::SeparatorOperator::Async } /
            specific_operator(";") { ast::SeparatorOperator::Sequence }

        rule separator() -> Option<ast::SeparatorOperator> =
            s:separator_op() linebreak() { Some(s) } /
            newline_list() { None }

        rule sequential_sep() -> () =
            specific_operator(";") linebreak() /
            newline_list()

        //
        // Token interpretation
        //

        rule word() -> &'input str =
            !reserved_word() [Token::Word(w, _)] { w.as_ref() }

        rule reserved_word() -> &'input str =
            t:reserved_word_token() { t.to_str() }

        rule reserved_word_token() -> &'input Token =
            specific_word("!") /
            specific_word("{") /
            specific_word("}") /
            specific_word("case") /
            specific_word("do") /
            specific_word("done") /
            specific_word("elif") /
            specific_word("else") /
            specific_word("esac") /
            specific_word("fi") /
            specific_word("for") /
            specific_word("if") /
            specific_word("in") /
            specific_word("then") /
            specific_word("until") /
            specific_word("while") /

            // N.B. bash also treats the following as reserved.
            specific_word("[[") /
            specific_word("]]") /
            specific_word("function") /
            specific_word("select")

        rule newline() -> () = quiet! {
            specific_operator("\n") { () }
        }

        rule assignment_word() -> (String, String) =
            // TODO: implement assignment_word more accurately.
            [Token::Word(x, _) if x.find('=').is_some()] {
                let (first, second) = x.split_once('=').unwrap();
                (first.to_owned(), second.to_owned())
            }

        rule io_number() -> u32 =
            // TODO: implement io_number more accurately.
            [Token::Word(w, _)] {? w.parse().or(Err("io_number u32")) }

        //
        // Helpers
        //
        rule specific_operator(expected: &str) -> &'input Token =
            [Token::Operator(w, _) if w.as_str() == expected]

        rule specific_word(expected: &str) -> &'input Token =
            [Token::Word(w, _) if w.as_str() == expected]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_case() -> Result<()> {
        let input = r#"\
case x in
x)
    echo y;;
esac\
"#;

        let tokens = tokenize(input)?;
        let command = super::token_parser::case_clause(&tokens)?;

        assert_eq!(command.cases.len(), 1);
        assert_eq!(command.cases[0].patterns.len(), 1);
        assert_eq!(command.cases[0].patterns[0], "x");

        Ok(())
    }

    #[test]
    fn parse_case_ns() -> Result<()> {
        let input = r#"\
case x in
x)
    echo y
esac\
"#;

        let tokens = tokenize(input)?;
        let command = super::token_parser::case_clause(&tokens)?;

        assert_eq!(command.cases.len(), 1);
        assert_eq!(command.cases[0].patterns.len(), 1);
        assert_eq!(command.cases[0].patterns[0], "x");

        Ok(())
    }

    fn tokenize(input: &str) -> Result<Tokens> {
        let mut reader = std::io::BufReader::new(input.as_bytes());
        let mut tokenizer = crate::tokenizer::Tokenizer::new(&mut reader);

        let mut tokens = vec![];
        loop {
            if let Some(token) = tokenizer.next_token()?.token {
                tokens.push(token);
            } else {
                break;
            }
        }

        Ok(Tokens { tokens })
    }
}
