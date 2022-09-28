#[derive(Clone, Debug)]
pub struct Program {
    pub complete_commands: Vec<CompleteCommand>,
}

pub type CompleteCommand = Vec<CompleteCommandItem>;
pub type CompleteCommandItem = (AndOrList, SeparatorOperator);

#[derive(Clone, Debug)]
pub enum SeparatorOperator {
    Async,
    Sequence,
}

#[derive(Clone, Debug)]
pub struct AndOrList {
    pub first: Pipeline,
    pub additional: Vec<AndOr>,
}

#[derive(Clone, Debug)]
pub enum AndOr {
    And(Pipeline),
    Or(Pipeline),
}

#[derive(Clone, Debug)]
pub struct Pipeline {
    pub bang: bool,
    pub seq: Vec<Command>,
}

#[derive(Clone, Debug)]
pub enum Command {
    Simple(SimpleCommand),
    Compound(CompoundCommand, Option<RedirectList>),
    Function(FunctionDefinition),
}

#[derive(Clone, Debug)]
pub enum CompoundCommand {
    BraceGroup(BraceGroupCommand),
    Subshell(SubshellCommand),
    ForClause(ForClauseCommand),
    CaseClause(CaseClauseCommand),
    IfClause(IfClauseCommand),
    WhileClause(WhileClauseCommand),
    UntilClause(UntilClauseCommand),
}

pub type SubshellCommand = CompoundList;

#[derive(Clone, Debug)]
pub struct ForClauseCommand {
    pub variable_name: String,
    pub values: Option<Vec<String>>,
    pub body: DoGroupCommand,
}

#[derive(Clone, Debug)]
pub struct CaseClauseCommand {
    pub value: String,
    pub cases: Vec<CaseItem>,
}

pub type CompoundList = Vec<CompoundListItem>;
pub type CompoundListItem = (AndOrList, SeparatorOperator);

#[derive(Clone, Debug)]
pub struct IfClauseCommand {
    pub condition: CompoundList,
    pub then: CompoundList,
    pub elses: Option<Vec<ElseClause>>,
}

#[derive(Clone, Debug)]
pub struct ElseClause {
    pub condition: Option<CompoundList>,
    pub body: CompoundList,
}

#[derive(Clone, Debug)]
pub struct CaseItem {
    pub patterns: Vec<String>,
    pub cmd: Option<CompoundList>,
}

pub type WhileClauseCommand = (CompoundList, DoGroupCommand);
pub type UntilClauseCommand = (CompoundList, DoGroupCommand);

#[derive(Clone, Debug)]
pub struct FunctionDefinition {
    pub fname: String,
    pub body: FunctionBody,
}

pub type FunctionBody = (CompoundCommand, Option<RedirectList>);
pub type BraceGroupCommand = CompoundList;
pub type DoGroupCommand = CompoundList;

#[derive(Clone, Debug)]
pub struct SimpleCommand {
    pub prefix: Option<CommandPrefix>,
    pub word_or_name: Option<String>,
    pub suffix: Option<CommandSuffix>,
}

pub type CommandPrefix = Vec<CommandPrefixOrSuffixItem>;
pub type CommandSuffix = Vec<CommandPrefixOrSuffixItem>;

#[derive(Clone, Debug)]
pub enum CommandPrefixOrSuffixItem {
    IoRedirect(IoRedirect),
    Word(String),
    AssignmentWord((String, String)),
}

pub type RedirectList = Vec<IoRedirect>;

#[derive(Clone, Debug)]
pub enum IoRedirect {
    File(Option<u32>, IoFileRedirectKind),
    Here(Option<u32>, IoHere),
}

#[derive(Clone, Debug)]
pub enum IoFileRedirectKind {
    Read,
    Write,
    Append,
    ReadAndWrite,
    Clobber,
    DuplicateInput,
    DuplicateOutput,
}

#[derive(Clone, Debug)]
pub struct IoHere {
    pub remove_tabs: bool,
    pub here_end: String,
}
