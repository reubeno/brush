use crate::ast::*;
use anyhow::Result;

pub trait AstTransformer {
    fn on_program(&mut self, program: &Program) -> Result<Program>;
    fn on_complete_command(
        &mut self,
        complete_command: &CompleteCommand,
    ) -> Result<CompleteCommand>;
    fn on_complete_command_item(
        &mut self,
        complete_command_item: &CompleteCommandItem,
    ) -> Result<CompleteCommandItem>;
    fn on_and_or_list(&mut self, and_or_list: &AndOrList) -> Result<AndOrList>;
    fn on_and_or(&mut self, and_or: &AndOr) -> Result<AndOr>;
    fn on_pipeline(&mut self, pipeline: &Pipeline) -> Result<Pipeline>;
    fn on_command(&mut self, command: &Command) -> Result<Command>;
    fn on_compound_command(
        &mut self,
        compound_command: &CompoundCommand,
    ) -> Result<CompoundCommand>;
    fn on_function_definition(
        &mut self,
        function_definition: &FunctionDefinition,
    ) -> Result<FunctionDefinition>;
    fn on_brace_group_command(
        &mut self,
        brace_group_command: &BraceGroupCommand,
    ) -> Result<BraceGroupCommand>;
    fn on_subshell_command(
        &mut self,
        subshell_command: &SubshellCommand,
    ) -> Result<SubshellCommand>;
    fn on_for_clause_command(
        &mut self,
        for_clause_command: &ForClauseCommand,
    ) -> Result<ForClauseCommand>;
    fn on_case_clause_command(
        &mut self,
        case_clause_command: &CaseClauseCommand,
    ) -> Result<CaseClauseCommand>;
    fn on_if_clause_command(
        &mut self,
        if_clause_command: &IfClauseCommand,
    ) -> Result<IfClauseCommand>;
    fn on_while_clause_command(
        &mut self,
        while_clause_command: &WhileClauseCommand,
    ) -> Result<WhileClauseCommand>;
    fn on_until_clause_command(
        &mut self,
        until_clause_command: &UntilClauseCommand,
    ) -> Result<UntilClauseCommand>;
    fn on_else_clause(&mut self, else_clause: &ElseClause) -> Result<ElseClause>;
    fn on_case_item(&mut self, case_item: &CaseItem) -> Result<CaseItem>;
    fn on_function_body(&mut self, function_body: &FunctionBody) -> Result<FunctionBody>;
    fn on_simple_command(&mut self, simple_command: &SimpleCommand) -> Result<SimpleCommand>;
    // fn on_assignment_word(&mut self, assignment_word: &AssignmentWord) -> Result<AssignmentWord>;
    // fn on_name(&mut self, name: &Name) -> Result<Name>;
    fn on_command_word(&mut self, word: &str) -> Result<String>;
    fn on_for_enumeree(&mut self, word: &str) -> Result<String>;
    fn on_case_value(&mut self, word: &str) -> Result<String>;
    fn on_for_variable_name(&mut self, word: &str) -> Result<String>;
    fn on_command_name(&mut self, word_or_name: &str) -> Result<String>;
    fn on_function_name(&mut self, function_name: &str) -> Result<String>;
}

pub fn transform_program<T: AstTransformer>(
    program: &Program,
    transformer: &mut T,
) -> Result<Program> {
    let inner = Program {
        complete_commands: program
            .complete_commands
            .iter()
            .map(|cc| transform_complete_command(cc, transformer))
            .into_iter()
            .collect::<Result<Vec<_>>>()?,
    };
    transformer.on_program(&inner)
}

fn transform_complete_command<T: AstTransformer>(
    complete_command: &CompleteCommand,
    transformer: &mut T,
) -> Result<CompleteCommand> {
    let inner = complete_command
        .iter()
        .map(|item| transform_complete_command_item(item, transformer))
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    transformer.on_complete_command(&inner)
}

fn transform_complete_command_item<T: AstTransformer>(
    item: &CompleteCommandItem,
    transformer: &mut T,
) -> Result<CompleteCommandItem> {
    let (ao_list, sep_op) = item;
    let inner = (transform_and_or_list(ao_list, transformer)?, sep_op.clone());
    transformer.on_complete_command_item(&inner)
}

fn transform_and_or_list<T: AstTransformer>(
    ao_list: &AndOrList,
    transformer: &mut T,
) -> Result<AndOrList> {
    let inner = AndOrList {
        first: transform_pipeline(&ao_list.first, transformer)?,
        additional: ao_list
            .additional
            .iter()
            .map(|ao| transform_and_or(ao, transformer))
            .into_iter()
            .collect::<Result<Vec<_>>>()?,
    };
    transformer.on_and_or_list(&inner)
}

fn transform_and_or<T: AstTransformer>(and_or: &AndOr, transformer: &mut T) -> Result<AndOr> {
    let inner = match and_or {
        AndOr::And(a) => AndOr::And(transform_pipeline(a, transformer)?),
        AndOr::Or(o) => AndOr::Or(transform_pipeline(o, transformer)?),
    };
    transformer.on_and_or(&inner)
}

fn transform_pipeline<T: AstTransformer>(
    pipeline: &Pipeline,
    transformer: &mut T,
) -> Result<Pipeline> {
    let inner = Pipeline {
        bang: pipeline.bang,
        seq: pipeline
            .seq
            .iter()
            .map(|s| transform_command(s, transformer))
            .into_iter()
            .collect::<Result<Vec<_>>>()?,
    };
    transformer.on_pipeline(&inner)
}

fn transform_command<T: AstTransformer>(command: &Command, transformer: &mut T) -> Result<Command> {
    let inner = match command {
        Command::Simple(s) => Command::Simple(transform_simple_command(s, transformer)?),
        Command::Compound(c, optional_rs) => Command::Compound(
            transform_compound_command(c, transformer)?,
            match optional_rs {
                Some(rs) => Some(
                    rs.iter()
                        .map(|r| transform_io_redirect(r, transformer))
                        .into_iter()
                        .collect::<Result<Vec<_>>>()?,
                ),
                None => None,
            },
        ),
        Command::Function(f) => Command::Function(transform_function_definition(f, transformer)?),
    };
    transformer.on_command(&inner)
}

fn transform_simple_command<T: AstTransformer>(
    simple_command: &SimpleCommand,
    transformer: &mut T,
) -> Result<SimpleCommand> {
    let inner = SimpleCommand {
        prefix: match &simple_command.prefix {
            Some(p) => Some(transform_command_prefix_or_suffix(p, transformer)?),
            None => None,
        },
        word_or_name: match &simple_command.word_or_name {
            Some(won) => Some(transform_command_name(won, transformer)?),
            None => None,
        },
        suffix: match &simple_command.suffix {
            Some(p) => Some(transform_command_prefix_or_suffix(p, transformer)?),
            None => None,
        },
    };
    transformer.on_simple_command(&inner)
}

fn transform_command_prefix_or_suffix<T: AstTransformer>(
    command_prefix_or_suffix: &Vec<CommandPrefixOrSuffixItem>,
    transformer: &mut T,
) -> Result<CommandPrefix> {
    command_prefix_or_suffix
        .iter()
        .map(|i| transform_command_prefix_or_suffix_item(i, transformer))
        .into_iter()
        .collect()
}

fn transform_command_prefix_or_suffix_item<T: AstTransformer>(
    command_prefix_or_suffix_item: &CommandPrefixOrSuffixItem,
    transformer: &mut T,
) -> Result<CommandPrefixOrSuffixItem> {
    match &command_prefix_or_suffix_item {
        CommandPrefixOrSuffixItem::IoRedirect(r) => Ok(CommandPrefixOrSuffixItem::IoRedirect(
            transform_io_redirect(r, transformer)?,
        )),
        CommandPrefixOrSuffixItem::Word(w) => Ok(CommandPrefixOrSuffixItem::Word(
            transform_command_word(w, transformer)?,
        )),
        CommandPrefixOrSuffixItem::AssignmentWord((name, value)) => {
            Ok(CommandPrefixOrSuffixItem::AssignmentWord((
                transform_command_word(name, transformer)?,
                transform_command_word(value, transformer)?,
            )))
        }
    }
}

fn transform_function_definition<T: AstTransformer>(
    function_definition: &FunctionDefinition,
    transformer: &mut T,
) -> Result<FunctionDefinition> {
    let inner = FunctionDefinition {
        fname: transform_function_name(&function_definition.fname, transformer)?,
        body: transform_function_body(&function_definition.body, transformer)?,
    };
    transformer.on_function_definition(&inner)
}

fn transform_function_body<T: AstTransformer>(
    function_body: &FunctionBody,
    transformer: &mut T,
) -> Result<FunctionBody> {
    let (cc, optional_rs) = function_body;

    Ok((
        transform_compound_command(cc, transformer)?,
        match optional_rs {
            Some(rs) => Some(
                rs.iter()
                    .map(|r| transform_io_redirect(r, transformer))
                    .into_iter()
                    .collect::<Result<Vec<_>>>()?,
            ),
            None => None,
        },
    ))
}

fn transform_compound_command<T: AstTransformer>(
    compound_command: &CompoundCommand,
    transformer: &mut T,
) -> Result<CompoundCommand> {
    let inner = match compound_command {
        CompoundCommand::BraceGroup(b) => {
            CompoundCommand::BraceGroup(transform_brace_group(b, transformer)?)
        }
        CompoundCommand::Subshell(s) => {
            CompoundCommand::Subshell(transform_subshell(s, transformer)?)
        }
        CompoundCommand::ForClause(f) => {
            CompoundCommand::ForClause(transform_for_clause(f, transformer)?)
        }
        CompoundCommand::CaseClause(c) => {
            CompoundCommand::CaseClause(transform_case_clause(c, transformer)?)
        }
        CompoundCommand::IfClause(i) => {
            CompoundCommand::IfClause(transform_if_clause(i, transformer)?)
        }
        CompoundCommand::WhileClause(w) => {
            CompoundCommand::WhileClause(transform_while_clause(w, transformer)?)
        }
        CompoundCommand::UntilClause(u) => {
            CompoundCommand::UntilClause(transform_until_clause(u, transformer)?)
        }
    };
    transformer.on_compound_command(&inner)
}

fn transform_brace_group<T: AstTransformer>(
    command: &BraceGroupCommand,
    transformer: &mut T,
) -> Result<BraceGroupCommand> {
    let inner = transform_compound_list(command, transformer)?;
    transformer.on_brace_group_command(inner.as_ref())
}

fn transform_subshell<T: AstTransformer>(
    command: &SubshellCommand,
    transformer: &mut T,
) -> Result<SubshellCommand> {
    let inner = transform_compound_list(command, transformer)?;
    transformer.on_subshell_command(inner.as_ref())
}

fn transform_for_clause<T: AstTransformer>(
    command: &ForClauseCommand,
    transformer: &mut T,
) -> Result<ForClauseCommand> {
    let inner = ForClauseCommand {
        variable_name: transform_for_variable_name(command.variable_name.as_ref(), transformer)?,
        values: match &command.values {
            Some(e) => Some(
                e.iter()
                    .map(|e| transform_for_enumeree(e, transformer))
                    .into_iter()
                    .collect::<Result<Vec<_>>>()?,
            ),
            None => None,
        },
        body: transform_do_group_command(command.body.as_ref(), transformer)?,
    };

    transformer.on_for_clause_command(&inner)
}

fn transform_case_clause<T: AstTransformer>(
    command: &CaseClauseCommand,
    transformer: &mut T,
) -> Result<CaseClauseCommand> {
    let inner = CaseClauseCommand {
        value: transform_case_value(command.value.as_ref(), transformer)?,
        cases: command
            .cases
            .iter()
            .map(|i| transform_case_item(i, transformer))
            .collect::<Result<Vec<_>>>()?,
    };

    transformer.on_case_clause_command(&inner)
}

fn transform_case_item<T: AstTransformer>(
    item: &CaseItem,
    transformer: &mut T,
) -> Result<CaseItem> {
    let inner = CaseItem {
        patterns: item.patterns.clone(),
        cmd: match &item.cmd {
            Some(c) => Some(transform_compound_list(c, transformer)?),
            None => None,
        },
    };

    transformer.on_case_item(&inner)
}

fn transform_if_clause<T: AstTransformer>(
    command: &IfClauseCommand,
    transformer: &mut T,
) -> Result<IfClauseCommand> {
    let inner = IfClauseCommand {
        condition: transform_compound_list(&command.condition, transformer)?,
        then: transform_compound_list(&command.then, transformer)?,
        elses: match &command.elses {
            Some(es) => Some(
                es.iter()
                    .map(|e| transform_else_clause(e, transformer))
                    .into_iter()
                    .collect::<Result<Vec<_>>>()?,
            ),
            None => None,
        },
    };

    transformer.on_if_clause_command(&inner)
}

fn transform_else_clause<T: AstTransformer>(
    clause: &ElseClause,
    transformer: &mut T,
) -> Result<ElseClause> {
    let inner = ElseClause {
        condition: match &clause.condition {
            Some(c) => Some(transform_compound_list(c, transformer)?),
            None => None,
        },
        body: transform_compound_list(&clause.body, transformer)?,
    };

    transformer.on_else_clause(&inner)
}

fn transform_while_clause<T: AstTransformer>(
    command: &WhileClauseCommand,
    transformer: &mut T,
) -> Result<WhileClauseCommand> {
    let (condition, body) = command;

    let inner = (
        transform_compound_list(condition, transformer)?,
        transform_do_group_command(body, transformer)?,
    );

    transformer.on_until_clause_command(&inner)
}

fn transform_until_clause<T: AstTransformer>(
    command: &UntilClauseCommand,
    transformer: &mut T,
) -> Result<UntilClauseCommand> {
    let (condition, body) = command;

    let inner = (
        transform_compound_list(condition, transformer)?,
        transform_do_group_command(body, transformer)?,
    );

    transformer.on_until_clause_command(&inner)
}

fn transform_do_group_command<T: AstTransformer>(
    command: &DoGroupCommand,
    transformer: &mut T,
) -> Result<DoGroupCommand> {
    transform_compound_list(command, transformer)
}

fn transform_compound_list<T: AstTransformer>(
    compound_list: &CompoundList,
    transformer: &mut T,
) -> Result<CompoundList> {
    compound_list
        .iter()
        .map(|i| transform_compound_list_item(i, transformer))
        .into_iter()
        .collect()
}

fn transform_compound_list_item<T: AstTransformer>(
    compound_list_item: &CompoundListItem,
    transformer: &mut T,
) -> Result<CompoundListItem> {
    let (aos, sep_op) = compound_list_item;
    Ok((transform_and_or_list(aos, transformer)?, sep_op.clone()))
}

fn transform_io_redirect<T: AstTransformer>(
    io_redirect: &IoRedirect,
    _transformer: &mut T,
) -> Result<IoRedirect> {
    // TODO: Consider allowing transformation.
    Ok(io_redirect.clone())
}

fn transform_for_variable_name<T: AstTransformer>(
    name: &str,
    transformer: &mut T,
) -> Result<String> {
    transformer.on_for_variable_name(name)
}

fn transform_case_value<T: AstTransformer>(value: &str, transformer: &mut T) -> Result<String> {
    transformer.on_case_value(value)
}

fn transform_for_enumeree<T: AstTransformer>(value: &str, transformer: &mut T) -> Result<String> {
    transformer.on_for_enumeree(value)
}

fn transform_command_word<T: AstTransformer>(word: &str, transformer: &mut T) -> Result<String> {
    transformer.on_command_word(word)
}

fn transform_command_name<T: AstTransformer>(
    word_or_name: &str,
    transformer: &mut T,
) -> Result<String> {
    transformer.on_command_name(word_or_name)
}

fn transform_function_name<T: AstTransformer>(
    function_name: &str,
    transformer: &mut T,
) -> Result<String> {
    transformer.on_function_name(function_name)
}
