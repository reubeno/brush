use anyhow::Result;
use log::debug;

use crate::ast::{self, SeparatorOperator};
use crate::tokenizer::{
    SourcePosition, Token, TokenEndReason, Tokenizer, TokenizerOptions, Tokens,
};

pub enum ParseResult {
    Program(ast::Program),
    ParseError(Option<Token>),
    TokenizerError {
        message: String,
        position: Option<SourcePosition>,
    },
}

#[derive(Clone, Debug, Default)]
pub struct ParserOptions {
    pub enable_extended_globbing: bool,
    pub posix_mode: bool,
}

pub struct Parser<R> {
    reader: R,
    options: ParserOptions,
}

impl<R: std::io::BufRead> Parser<R> {
    pub fn new(reader: R, options: &ParserOptions) -> Self {
        Parser {
            reader,
            options: options.clone(),
        }
    }

    pub fn parse(&mut self, stop_on_unescaped_newline: bool) -> Result<ParseResult> {
        //
        // References:
        //   * https://www.gnu.org/software/bash/manual/bash.html#Shell-Syntax
        //   * https://mywiki.wooledge.org/BashParser
        //   * https://aosabook.org/en/v1/bash.html
        //   * https://pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html
        //

        let mut tokenizer = Tokenizer::new(
            &mut self.reader,
            &TokenizerOptions {
                enable_extended_globbing: self.options.enable_extended_globbing,
                posix_mode: self.options.posix_mode,
            },
        );

        let mut tokens = vec![];
        loop {
            let result = match tokenizer.next_token() {
                Ok(result) => result,
                Err(e) => {
                    return Ok(ParseResult::TokenizerError {
                        message: e.to_string(),
                        position: tokenizer.current_location(),
                    });
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

        parse_tokens_impl(&tokens)
    }
}

pub fn parse_tokens(tokens: &Vec<Token>) -> Result<ast::Program> {
    match parse_tokens_impl(tokens)? {
        ParseResult::Program(prog) => Ok(prog),
        ParseResult::ParseError(_) => Err(anyhow::anyhow!("parse error")),
        ParseResult::TokenizerError { .. } => Err(anyhow::anyhow!("tokenizer error")),
    }
}

fn parse_tokens_impl(tokens: &Vec<Token>) -> Result<ParseResult> {
    let parse_result = token_parser::program(&Tokens { tokens });

    let result = match parse_result {
        Ok(program) => ParseResult::Program(program),
        Err(parse_error) => {
            debug!("Parse error: {:?}", parse_error);

            let approx_token_index = parse_error.location;

            let token_near_error = if approx_token_index < tokens.len() {
                Some(tokens[approx_token_index].clone())
            } else {
                None
            };

            ParseResult::ParseError(token_near_error)
        }
    };

    if log::log_enabled!(log::Level::Debug) {
        if let ParseResult::Program(program) = &result {
            debug!("PROG: {:?}", program);
        }
    }

    Ok(result)
}

impl<'a> peg::Parse for Tokens<'a> {
    type PositionRepr = usize;

    fn start(&self) -> usize {
        0
    }

    fn is_eof(&self, p: usize) -> bool {
        p >= self.tokens.len()
    }

    fn position_repr(&self, p: usize) -> Self::PositionRepr {
        p
    }
}

impl<'a> peg::ParseElem<'a> for Tokens<'a> {
    type Element = &'a Token;

    fn parse_elem(&'a self, pos: usize) -> peg::RuleResult<Self::Element> {
        match self.tokens.get(pos) {
            Some(c) => peg::RuleResult::Matched(pos + 1, c),
            None => peg::RuleResult::Failed,
        }
    }
}

impl<'a> peg::ParseSlice<'a> for Tokens<'a> {
    type Slice = String;

    fn parse_slice(&'a self, start: usize, end: usize) -> Self::Slice {
        self.tokens[start..end]
            .iter()
            .map(|t| t.to_str())
            .collect::<Vec<_>>()
            .join("")
    }
}

peg::parser! {
    grammar token_parser<'a>() for Tokens<'a> {
        pub(crate) rule program() -> ast::Program =
            linebreak() c:complete_commands() linebreak() { ast::Program { complete_commands: c } } /
            linebreak() { ast::Program { complete_commands: vec![] } }

        rule complete_commands() -> Vec<ast::CompleteCommand> =
            c:complete_command() ++ newline_list()

        rule complete_command() -> ast::CompleteCommand =
            first:and_or() remainder:(s:separator_op() l:and_or() { (s, l) })* last_sep:separator_op()? {
                let mut and_ors = vec![first];
                let mut seps = vec![];

                for (sep, ao) in remainder.into_iter() {
                    seps.push(sep);
                    and_ors.push(ao);
                }

                // N.B. We default to synchronous if no separator op is given.
                seps.push(last_sep.unwrap_or(SeparatorOperator::Sequence));

                let mut items = vec![];
                for (i, ao) in and_ors.into_iter().enumerate() {
                    items.push((ao, seps[i].clone()));
                }

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
            // TODO: Don't allow ExtendedTest in POSIX compliance mode.
            c:extended_test_command() { ast::Command::ExtendedTest(c) } /
            expected!("command")

        // N.B. The arithmetic command is a non-sh extension.
        // TODO: Don't allow the arithmetic command in sh mode.
        rule compound_command() -> ast::CompoundCommand =
            a:arithmetic_command() { ast::CompoundCommand::Arithmetic(a) } /
            b:brace_group() { ast::CompoundCommand::BraceGroup(b) } /
            s:subshell() { ast::CompoundCommand::Subshell(s) } /
            f:for_clause() { ast::CompoundCommand::ForClause(f) } /
            c:case_clause() { ast::CompoundCommand::CaseClause(c) } /
            i:if_clause() { ast::CompoundCommand::IfClause(i) } /
            w:while_clause() { ast::CompoundCommand::WhileClause(w) } /
            u:until_clause() { ast::CompoundCommand::UntilClause(u) } /
            expected!("compound command")

        // N.B. This is not supported in sh.
        rule arithmetic_command() -> ast::ArithmeticCommand =
            specific_operator("(") specific_operator("(") expr:arithmetic_expression() arithmetic_end() {
                ast::ArithmeticCommand { expr }
            }

        rule arithmetic_expression() -> ast::ArithmeticExpr =
            raw_expr:$((!arithmetic_end() [_])*) {?
                crate::arithmetic::parse_arithmetic_expression(raw_expr.as_str()).or(Err("arithmetic expr"))
            }

        rule arithmetic_end() -> () =
            specific_operator(")") specific_operator(")") {}

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

        rule extended_test_command() -> ast::ExtendedTestExpr =
            specific_word("[[") e:extended_test_expression() specific_word("]]") { e }

        rule extended_test_expression() -> ast::ExtendedTestExpr = precedence! {
            left:(@) specific_operator("||") right:@ { ast::ExtendedTestExpr::Or(Box::from(left), Box::from(right)) }
            --
            left:(@) specific_operator("&&") right:@ { ast::ExtendedTestExpr::And(Box::from(left), Box::from(right)) }
            --
            specific_word("!") e:@ { ast::ExtendedTestExpr::Not(Box::from(e)) }
            --
            specific_operator("(") e:extended_test_expression() specific_operator(")") { ast::ExtendedTestExpr::Parenthesized(Box::from(e)) }
            --
            left:word() specific_word("-ef") right:word() { ast::ExtendedTestExpr::BinaryTest(ast::BinaryPredicate::FilesReferToSameDeviceAndInodeNumbers, ast::Word::from(left), ast::Word::from(right)) }
            left:word() specific_word("-eq") right:word() { ast::ExtendedTestExpr::BinaryTest(ast::BinaryPredicate::ArithmeticEqualTo, ast::Word::from(left), ast::Word::from(right)) }
            left:word() specific_word("-ge") right:word() { ast::ExtendedTestExpr::BinaryTest(ast::BinaryPredicate::ArithmeticGreaterThanOrEqualTo, ast::Word::from(left), ast::Word::from(right)) }
            left:word() specific_word("-gt") right:word() { ast::ExtendedTestExpr::BinaryTest(ast::BinaryPredicate::ArithmeticGreaterThan, ast::Word::from(left), ast::Word::from(right)) }
            left:word() specific_word("-le") right:word() { ast::ExtendedTestExpr::BinaryTest(ast::BinaryPredicate::ArithmeticLessThanOrEqualTo, ast::Word::from(left), ast::Word::from(right)) }
            left:word() specific_word("-lt") right:word() { ast::ExtendedTestExpr::BinaryTest(ast::BinaryPredicate::ArithmeticLessThan, ast::Word::from(left), ast::Word::from(right)) }
            left:word() specific_word("-ne") right:word() { ast::ExtendedTestExpr::BinaryTest(ast::BinaryPredicate::ArithmeticNotEqualTo, ast::Word::from(left), ast::Word::from(right)) }
            left:word() specific_word("-nt") right:word() { ast::ExtendedTestExpr::BinaryTest(ast::BinaryPredicate::LeftFileIsNewerOrExistsWhenRightDoesNot, ast::Word::from(left), ast::Word::from(right)) }
            left:word() specific_word("-ot") right:word() { ast::ExtendedTestExpr::BinaryTest(ast::BinaryPredicate::LeftFileIsOlderOrDoesNotExistWhenRightDoes, ast::Word::from(left), ast::Word::from(right)) }
            left:word() specific_word("==") right:word()  { ast::ExtendedTestExpr::BinaryTest(ast::BinaryPredicate::StringsAreEqual, ast::Word::from(left), ast::Word::from(right)) }
            left:word() specific_word("=") right:word()   { ast::ExtendedTestExpr::BinaryTest(ast::BinaryPredicate::StringsAreEqual, ast::Word::from(left), ast::Word::from(right)) }
            left:word() specific_word("!=") right:word()  { ast::ExtendedTestExpr::BinaryTest(ast::BinaryPredicate::StringsNotEqual, ast::Word::from(left), ast::Word::from(right)) }
            left:word() specific_word("<") right:word()   { ast::ExtendedTestExpr::BinaryTest(ast::BinaryPredicate::LeftSortsBeforeRight, ast::Word::from(left), ast::Word::from(right)) }
            left:word() specific_word(">") right:word()   { ast::ExtendedTestExpr::BinaryTest(ast::BinaryPredicate::LeftSortsAfterRight, ast::Word::from(left), ast::Word::from(right)) }
            --
            specific_word("-a") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExists, ast::Word::from(f)) }
            specific_word("-b") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndIsBlockSpecialFile, ast::Word::from(f)) }
            specific_word("-c") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndIsCharSpecialFile, ast::Word::from(f)) }
            specific_word("-d") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndIsDir, ast::Word::from(f)) }
            specific_word("-e") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExists, ast::Word::from(f)) }
            specific_word("-f") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndIsRegularFile, ast::Word::from(f)) }
            specific_word("-g") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndIsSetgid, ast::Word::from(f)) }
            specific_word("-h") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndIsSymlink, ast::Word::from(f)) }
            specific_word("-k") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndHasStickyBit, ast::Word::from(f)) }
            specific_word("-n") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::StringHasNonZeroLength, ast::Word::from(f)) }
            specific_word("-o") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::ShellOptionEnabled, ast::Word::from(f)) }
            specific_word("-p") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndIsFifo, ast::Word::from(f)) }
            specific_word("-r") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndIsReadable, ast::Word::from(f)) }
            specific_word("-s") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndIsNotZeroLength, ast::Word::from(f)) }
            specific_word("-t") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FdIsOpenTerminal, ast::Word::from(f)) }
            specific_word("-u") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndIsSetuid, ast::Word::from(f)) }
            specific_word("-v") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::ShellVariableIsSetAndAssigned, ast::Word::from(f)) }
            specific_word("-w") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndIsWritable, ast::Word::from(f)) }
            specific_word("-x") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndIsExecutable, ast::Word::from(f)) }
            specific_word("-z") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::StringHasZeroLength, ast::Word::from(f)) }
            specific_word("-G") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndOwnedByEffectiveGroupId, ast::Word::from(f)) }
            specific_word("-L") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndIsSymlink, ast::Word::from(f)) }
            specific_word("-N") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndModifiedSinceLastRead, ast::Word::from(f)) }
            specific_word("-O") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndOwnedByEffectiveUserId, ast::Word::from(f)) }
            specific_word("-R") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::ShellVariableIsSetAndNameRef, ast::Word::from(f)) }
            specific_word("-S") f:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::FileExistsAndIsSocket, ast::Word::from(f)) }
            --
            w:word() { ast::ExtendedTestExpr::UnaryTest(ast::UnaryPredicate::StringHasNonZeroLength, ast::Word::from(w)) }
        }

        rule name() -> &'input str =
            w:[Token::Word(_, _)] { w.to_str() }

        rule _in() -> () =
            specific_word("in") { }

        rule wordlist() -> Vec<ast::Word> =
            (w:word() { ast::Word::from(w) })+

        pub(crate) rule case_clause() -> ast::CaseClauseCommand =
            specific_word("case") w:word() linebreak() _in() linebreak() c:case_list() specific_word("esac") {
                ast::CaseClauseCommand { value: ast::Word::from(w), cases: c }
            } /
            specific_word("case") w:word() linebreak() _in() linebreak() c:case_list_ns() specific_word("esac") {
                ast::CaseClauseCommand { value: ast::Word::from(w), cases: c }
            } /
            specific_word("case") w:word() linebreak() _in() linebreak() specific_word("esac") {
                ast::CaseClauseCommand{ value: ast::Word::from(w), cases: vec![] }
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

        rule pattern() -> Vec<ast::Word> =
            (w:word() { ast::Word::from(w) }) ++ specific_operator("|")

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
            specific_word("function")? fname:fname() specific_operator("(") specific_operator(")") linebreak() body:function_body() {
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
            prefix:cmd_prefix() word_or_name:cmd_word() suffix:cmd_suffix()? { ast::SimpleCommand { prefix: Some(prefix), word_or_name: Some(ast::Word::from(word_or_name)), suffix } } /
            prefix:cmd_prefix() { ast::SimpleCommand { prefix: Some(prefix), word_or_name: None, suffix: None } } /
            word_or_name:cmd_name() suffix:cmd_suffix()? { ast::SimpleCommand { prefix: None, word_or_name: Some(ast::Word::from(word_or_name)), suffix } } /
            expected!("simple command")

        rule cmd_name() -> &'input Token =
            word()

        rule cmd_word() -> &'input Token =
            !assignment_word() w:word() { w }

        rule cmd_prefix() -> ast::CommandPrefix =
            (i:io_redirect() { ast::CommandPrefixOrSuffixItem::IoRedirect(i) } /
                w:assignment_word() { ast::CommandPrefixOrSuffixItem::AssignmentWord(w) })+

        rule cmd_suffix() -> ast::CommandSuffix =
            (i:io_redirect() { ast::CommandPrefixOrSuffixItem::IoRedirect(i) } /
                w:word() { ast::CommandPrefixOrSuffixItem::Word(ast::Word::from(w)) })+

        rule redirect_list() -> ast::RedirectList =
            io_redirect()+ /
            expected!("redirect list")

        // N.B. here strings are extensions to the POSIX standard.
        // TODO: don't support here strings in sh mode
        rule io_redirect() -> ast::IoRedirect =
            n:io_number()? f:io_file() {
                let (kind, target) = f;
                ast::IoRedirect::File(n, kind, target)
            } /
            n:io_number()? specific_operator("<<<") w:word() { ast::IoRedirect::HereString(n, ast::Word::from(w)) } /
            n:io_number()? h:io_here() { ast::IoRedirect::HereDocument(n, h) } /
            expected!("I/O redirect")

        // N.B. Process substitution forms are extensions to the POSIX standard.
        // TODO: don't support process substitution in sh mode
        rule io_file() -> (ast::IoFileRedirectKind, ast::IoFileRedirectTarget) =
            specific_operator("<") s:subshell() { (ast::IoFileRedirectKind::Read, ast::IoFileRedirectTarget::ProcessSubstitution(s)) } /
            specific_operator("<")  f:io_filename() { (ast::IoFileRedirectKind::Read, f) } /
            specific_operator("<&") f:io_filename_or_fd() { (ast::IoFileRedirectKind::DuplicateInput, f) } /
            specific_operator(">") s:subshell() { (ast::IoFileRedirectKind::Write, ast::IoFileRedirectTarget::ProcessSubstitution(s)) } /
            specific_operator(">")  f:io_filename() { (ast::IoFileRedirectKind::Write, f) } /
            specific_operator(">&") f:io_filename_or_fd() { (ast::IoFileRedirectKind::DuplicateOutput, f) } /
            specific_operator(">>") f:io_filename() { (ast::IoFileRedirectKind::Append, f) } /
            specific_operator("<>") f:io_filename() { (ast::IoFileRedirectKind::ReadAndWrite, f) } /
            specific_operator(">|") f:io_filename() { (ast::IoFileRedirectKind::Clobber, f) }

        rule io_filename_or_fd() -> ast::IoFileRedirectTarget =
            fd:io_fd() { ast::IoFileRedirectTarget::Fd(fd) } /
            io_filename()

        rule io_fd() -> u32 =
            w:[Token::Word(_, _)] {? w.to_str().parse().or(Err("io_fd u32")) }

        rule io_filename() -> ast::IoFileRedirectTarget =
            f:filename() { ast::IoFileRedirectTarget::Filename(ast::Word::from(f)) }

        rule filename() -> &'input Token =
            word()

        rule io_here() -> ast::IoHereDocument =
            specific_operator("<<") here_end:here_end() newline() doc:[_] { ast::IoHereDocument { remove_tabs: false, here_end: ast::Word::from(here_end), doc: ast::Word::from(doc) } } /
            specific_operator("<<-") here_end:here_end() newline() doc:[_] { ast::IoHereDocument { remove_tabs: true, here_end: ast::Word::from(here_end), doc: ast::Word::from(doc) } }

        rule here_end() -> &'input Token =
            word()

        rule newline_list() -> () =
            newline()* {}

        // N.B. We don't need to add a '?' to the invocation of the newline_list()
        // rule because it already allows 0 newlines.
        rule linebreak() -> () =
            quiet! {
                newline_list() {}
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

        rule word() -> &'input Token =
            !reserved_word() w:[Token::Word(_, _)] { w }

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
            // TODO: Disable these in POSIX compliance mode.
            specific_word("[[") /
            specific_word("]]") /
            specific_word("function") /
            specific_word("select")

        rule newline() -> () = quiet! {
            specific_operator("\n") {}
        }

        rule assignment_word() -> ast::Assignment =
            // TODO: implement assignment_word more accurately, i.e., check to make sure
            // the variable being assigned is a legitimate variable name.
            // TODO: make sure array syntax isn't present in sh mode
            [Token::Word(w, _)] specific_operator("(") elements:([Token::Word(e, _)] { e })* specific_operator(")") {?
                parse_array_assignment(w.as_str(), &elements)
            } /
            [Token::Word(w, _)] {?
                parse_assignment_word(w.as_str())
            }

        rule io_number() -> u32 =
            // TODO: implement io_number more accurately.
            w:[Token::Word(_, _)] {? w.to_str().parse().or(Err("io_number u32")) }

        //
        // Helpers
        //
        rule specific_operator(expected: &str) -> &'input Token =
            [Token::Operator(w, _) if w.as_str() == expected]

        rule specific_word(expected: &str) -> &'input Token =
            [Token::Word(w, _) if w.as_str() == expected]
    }
}

fn parse_assignment_word(word: &str) -> Result<ast::Assignment, &'static str> {
    let variable_name: String;
    let value;
    if let Some((first, second)) = word.split_once('=') {
        variable_name = first.to_owned();
        value = second.to_owned();
    } else {
        return Err("not assignment word");
    }

    Ok(ast::Assignment::Scalar {
        name: variable_name,
        value: ast::Word { value },
    })
}

fn parse_array_assignment(
    word: &str,
    elements: &[&String],
) -> Result<ast::Assignment, &'static str> {
    if let Some(variable_name) = word.strip_suffix('=') {
        if variable_name.contains('=') {
            Err("not assignment word")
        } else {
            Ok(ast::Assignment::Array {
                name: variable_name.to_owned(),
                values: elements
                    .iter()
                    .map(|e| ast::Word {
                        value: (*e).to_owned(),
                    })
                    .collect(),
            })
        }
    } else {
        Err("not assignment word")
    }
}

#[cfg(test)]
mod tests {
    use crate::tokenizer::tokenize_str;

    use super::*;

    #[test]
    fn parse_case() -> Result<()> {
        let input = r"\
case x in
x)
    echo y;;
esac\
";

        let tokens = tokenize_str(input)?;
        let command = super::token_parser::case_clause(&Tokens {
            tokens: tokens.as_slice(),
        })?;

        assert_eq!(command.cases.len(), 1);
        assert_eq!(command.cases[0].patterns.len(), 1);
        assert_eq!(command.cases[0].patterns[0].flatten(), "x");

        Ok(())
    }

    #[test]
    fn parse_case_ns() -> Result<()> {
        let input = r"\
case x in
x)
    echo y
esac\
";

        let tokens = tokenize_str(input)?;
        let command = super::token_parser::case_clause(&Tokens {
            tokens: tokens.as_slice(),
        })?;

        assert_eq!(command.cases.len(), 1);
        assert_eq!(command.cases[0].patterns.len(), 1);
        assert_eq!(command.cases[0].patterns[0].flatten(), "x");

        Ok(())
    }
}
