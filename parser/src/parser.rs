use anyhow::Result;
use log::debug;

use crate::ast::{self, SeparatorOperator};
use crate::tokenizer::{Token, TokenEndReason, Tokenizer, Tokens};

pub struct ParseResult {
    pub program: Option<ast::Program>,
    pub token_near_error: Option<String>,
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
            let result = tokenizer.next_token()?;
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
            Ok(program) => ParseResult {
                program: Some(program),
                token_near_error: None,
            },
            Err(parse_error) => {
                let approx_token_index = parse_error.location;

                let token_near_error;
                if approx_token_index < tokens.tokens.len() {
                    token_near_error = Some(tokens.tokens[approx_token_index].to_str().to_owned());
                } else {
                    token_near_error = None;
                }

                ParseResult {
                    program: None,
                    token_near_error,
                }
            }
        };

        if log::log_enabled!(log::Level::Debug) && result.program.is_some() {
            debug!("PROG: {:#?}", result.program);
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
            [Token::Operator(n) if n.as_str() == "&&"] linebreak() p:pipeline() { ast::AndOr::And(p) } /
            [Token::Operator(n) if n.as_str() == "||"] linebreak() p:pipeline() { ast::AndOr::Or(p) }


        rule pipeline() -> ast::Pipeline =
            bang:bang()? seq:pipe_sequence() { ast::Pipeline { bang: bang.is_some(), seq } }
        rule bang() -> bool =
            [Token::Operator(n) if n.as_str() == "!"] { true }

        rule pipe_sequence() -> Vec<ast::Command> =
            c:command() ++ ([Token::Operator(n) if n.as_str() == "|"] linebreak()) { c }

        rule command() -> ast::Command =
            c:simple_command() { ast::Command::Simple(c) } /
            c:compound_command() r:redirect_list()? { ast::Command::Compound(c, r) } /
            f:function_definition() { ast::Command::Function(f) }

        rule compound_command() -> ast::CompoundCommand =
            b:brace_group() { ast::CompoundCommand::BraceGroup(b) } /
            s:subshell() { ast::CompoundCommand::Subshell(s) } /
            f:for_clause() { ast::CompoundCommand::ForClause(f) } /
            c:case_clause() { ast::CompoundCommand::CaseClause(c) } /
            i:if_clause() { ast::CompoundCommand::IfClause(i) } /
            w:while_clause() { ast::CompoundCommand::WhileClause(w) } /
            u:until_clause() { ast::CompoundCommand::UntilClause(u) }

        rule subshell() -> ast::SubshellCommand =
            [Token::Operator(n) if n.as_str() == "("] c:compound_list() [Token::Operator(n) if n.as_str() == ")"] { c }

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
            [Token::Word(w) if w.as_str() == "for"] n:name() linebreak() _in() w:wordlist()? sequential_sep() d:do_group() {
                ast::ForClauseCommand { variable_name: n.to_owned(), values: w, body: d }
            } /
            [Token::Word(w) if w.as_str() == "for"] n:name() sequential_sep()? d:do_group() {
                ast::ForClauseCommand { variable_name: n.to_owned(), values: None, body: d }
            }

        rule name() -> &'input str =
            [Token::Word(w)] { w }

        rule _in() -> () =
            [Token::Word(w) if w.as_str() == "in"] { () }

        rule wordlist() -> Vec<String> =
            (w:word() { w.to_owned() })+

        rule case_clause() -> ast::CaseClauseCommand =
            [Token::Word(case_word) if case_word.as_str() == "case"] w:word() linebreak() _in() linebreak() c:case_list() [Token::Word(esac_word) if esac_word.as_str() == "esac"] {
                ast::CaseClauseCommand { value: w.to_owned(), cases: c }
            } /
            [Token::Word(case_word) if case_word.as_str() == "case"] w:word() linebreak() _in() linebreak() c:case_list_ns() [Token::Word(esac_word) if esac_word.as_str() == "esac"] {
                ast::CaseClauseCommand { value: w.to_owned(), cases: c }
            } /
            [Token::Word(case_word) if case_word.as_str() == "case"] w:word() linebreak() _in() linebreak() [Token::Word(esac_word) if esac_word.as_str() == "esac"] {
                ast::CaseClauseCommand{ value: w.to_owned(), cases: vec![] }
            }

        rule case_list_ns() -> Vec<ast::CaseItem> =
            first:case_list() last:case_item_ns() {
                let mut items = vec![];
                for item in first.into_iter() {
                    items.push(item);
                }
                items.push(last);
                items
            }

        rule case_list() -> Vec<ast::CaseItem> =
            c:case_item()+

        rule case_item_ns() -> ast::CaseItem =
            [Token::Word(w) if w.as_str() == "("]? p:pattern() [Token::Operator(n) if n.as_str() == ")"] linebreak() {
                ast::CaseItem { patterns: p, cmd: None }
            } /
            [Token::Word(w) if w.as_str() == "("]? p:pattern() [Token::Operator(n) if n.as_str() == ")"] c:compound_list() {
                ast::CaseItem { patterns: p, cmd: Some(c) }
            }

        rule case_item() -> ast::CaseItem =
            [Token::Word(w) if w.as_str() == "("]? p:pattern() [Token::Operator(n) if n.as_str() == ")"] linebreak() [Token::Operator(n) if n.as_str() == ";;"] linebreak() {
                ast::CaseItem { patterns: p, cmd: None }
            } /
            [Token::Word(w) if w.as_str() == "("]? p:pattern() [Token::Operator(n) if n.as_str() == ")"] c:compound_list() [Token::Operator(n) if n.as_str() == ";;"] linebreak() {
                ast::CaseItem { patterns: p, cmd: Some(c) }
            }

        rule pattern() -> Vec<String> =
            (w:word() { w.to_owned() }) ++ [Token::Operator(n) if n.as_str() == "|"]

        rule if_clause() -> ast::IfClauseCommand =
            [Token::Word(w) if w.as_str() == "if"] condition:compound_list() [Token::Word(w) if w.as_str() == "then"] then:compound_list() elses:else_part()? [Token::Word(w) if w.as_str() == "fi"] {
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
            [Token::Word(w) if w.as_str() == "elif"] condition:compound_list() [Token::Word(w) if w.as_str() == "then"] body:compound_list() {
                ast::ElseClause { condition: Some(condition), body }
            }

        rule _unconditional_else_part() -> ast::ElseClause =
            [Token::Word(w) if w.as_str() == "else"] body:compound_list() {
                ast::ElseClause { condition: None, body }
             }

        rule while_clause() -> ast::WhileClauseCommand =
            [Token::Word(w) if w.as_str() == "while"] c:compound_list() d:do_group() { (c, d) }

        rule until_clause() -> ast::UntilClauseCommand =
            [Token::Word(w) if w.as_str() == "until"] c:compound_list() d:do_group() { (c, d) }

        rule function_definition() -> ast::FunctionDefinition =
            fname:fname() [Token::Operator(n) if n.as_str() == "("] [Token::Operator(n) if n.as_str() == ")"] linebreak() body:function_body() {
                ast::FunctionDefinition { fname: fname.to_owned(), body }
            }

        rule function_body() -> ast::FunctionBody =
            c:compound_command() r:redirect_list()? { (c, r) }

        rule fname() -> &'input str =
            name()

        rule brace_group() -> ast::BraceGroupCommand =
            [Token::Operator(n) if n.as_str() == "{"] c:compound_list() [Token::Operator(n) if n.as_str() == "}"] { c }

        rule do_group() -> ast::DoGroupCommand =
            [Token::Word(w) if w.as_str() == "do"] c:compound_list() [Token::Word(w) if w.as_str() == "done"] { c }

        rule simple_command() -> ast::SimpleCommand =
            prefix:cmd_prefix() word_or_name:cmd_word() suffix:cmd_suffix()? { ast::SimpleCommand { prefix: Some(prefix), word_or_name: Some(word_or_name.to_owned()), suffix } } /
            prefix:cmd_prefix() { ast::SimpleCommand { prefix: Some(prefix), word_or_name: None, suffix: None } } /
            word_or_name:cmd_name() suffix:cmd_suffix()? { ast::SimpleCommand { prefix: None, word_or_name: Some(word_or_name.to_owned()), suffix } }

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
            io_redirect()+

        rule io_redirect() -> ast::IoRedirect =
            n:io_number()? f:io_file() {
                let (kind, filename) = f;
                ast::IoRedirect::File(n, kind, filename.to_owned())
            } /
            n:io_number()? h:io_here() { ast::IoRedirect::Here(n, h) }

        rule io_file() -> (ast::IoFileRedirectKind, &'input str) =
            [Token::Operator(o) if o.as_str() == "<"]  f:filename() { (ast::IoFileRedirectKind::Read, f) } /
            [Token::Operator(o) if o.as_str() == "<&"] f:filename() { (ast::IoFileRedirectKind::DuplicateInput, f) } /
            [Token::Operator(o) if o.as_str() == ">"]  f:filename() { (ast::IoFileRedirectKind::Write, f) } /
            [Token::Operator(o) if o.as_str() == ">&"] f:filename() { (ast::IoFileRedirectKind::DuplicateOutput, f) } /
            [Token::Operator(o) if o.as_str() == ">>"] f:filename() { (ast::IoFileRedirectKind::Append, f) } /
            [Token::Operator(o) if o.as_str() == "<>"] f:filename() { (ast::IoFileRedirectKind::ReadAndWrite, f) } /
            [Token::Operator(o) if o.as_str() == ">|"] f:filename() { (ast::IoFileRedirectKind::Clobber, f) }

        rule filename() -> &'input str =
            word()

        rule io_here() -> ast::IoHere =
            [Token::Operator(o) if o.as_str() == "<<"] here_end:here_end() { ast::IoHere { remove_tabs: false, here_end: here_end.to_owned() } } /
            [Token::Operator(o) if o.as_str() == "<<-"] here_end:here_end() { ast::IoHere { remove_tabs: true, here_end: here_end.to_owned() } }

        rule here_end() -> &'input str =
            word()

        rule newline_list() -> () =
            newline()* { () }

        rule linebreak() -> () =
            quiet! {
                newline_list()? { () }
            }

        rule separator_op() -> ast::SeparatorOperator =
            [Token::Operator(n) if n.as_str() == "&"] { ast::SeparatorOperator::Async } /
            [Token::Operator(n) if n.as_str() == ";"] { ast::SeparatorOperator::Sequence }

        rule separator() -> Option<ast::SeparatorOperator> =
            s:separator_op() linebreak() { Some(s) } /
            newline_list() { None }

        rule sequential_sep() -> () =
            [Token::Operator(n) if n.as_str() == ";"] linebreak() /
            newline_list()

        //
        // Token interpretation
        //

        rule word() -> &'input str =
            !reserved_word() [Token::Word(w)] { w.as_ref() }

        rule reserved_word() -> &'input str =
            t:reserved_word_token() { t.to_str() }

        rule reserved_word_token() -> &'input Token =
            [Token::Word(w) if w.as_str() == "!"] /
            [Token::Word(w) if w.as_str() == "{"] /
            [Token::Word(w) if w.as_str() == "}"] /
            [Token::Word(w) if w.as_str() == "case"] /
            [Token::Word(w) if w.as_str() == "do"] /
            [Token::Word(w) if w.as_str() == "done"] /
            [Token::Word(w) if w.as_str() == "elif"] /
            [Token::Word(w) if w.as_str() == "else"] /
            [Token::Word(w) if w.as_str() == "esac"] /
            [Token::Word(w) if w.as_str() == "fi"] /
            [Token::Word(w) if w.as_str() == "for"] /
            [Token::Word(w) if w.as_str() == "if"] /
            [Token::Word(w) if w.as_str() == "in"] /
            [Token::Word(w) if w.as_str() == "then"] /
            [Token::Word(w) if w.as_str() == "until"] /
            [Token::Word(w) if w.as_str() == "while"]

        rule newline() -> () = quiet! {
            [Token::Operator(n) if n.as_str() == "\n"] { () }
        }

        rule assignment_word() -> (String, String) =
            // TODO: implement assignment_word more accurately.
            [Token::Word(x) if x.find('=').is_some()] {
                let (first, second) = x.split_once('=').unwrap();
                (first.to_owned(), second.to_owned())
            }

        rule io_number() -> u32 =
            // TODO: implement io_number more accurately.
            [Token::Word(w)] {? w.parse().or(Err("u32")) }
    }
}
