use crate::tokenizer;

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
    ExtendedTest(ExtendedTestExpression),
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
    pub values: Option<Vec<Word>>,
    pub body: DoGroupCommand,
}

#[derive(Clone, Debug)]
pub struct CaseClauseCommand {
    pub value: Word,
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
    pub patterns: Vec<Word>,
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
    pub word_or_name: Option<Word>,
    pub suffix: Option<CommandSuffix>,
}

pub type CommandPrefix = Vec<CommandPrefixOrSuffixItem>;
pub type CommandSuffix = Vec<CommandPrefixOrSuffixItem>;

#[derive(Clone, Debug)]
pub enum CommandPrefixOrSuffixItem {
    IoRedirect(IoRedirect),
    Word(Word),
    AssignmentWord((String, Word)),
}

pub type RedirectList = Vec<IoRedirect>;

#[derive(Clone, Debug)]
pub enum IoRedirect {
    File(Option<u32>, IoFileRedirectKind, IoFileRedirectTarget),
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
pub enum IoFileRedirectTarget {
    Filename(Word),
    Fd(u32),
}

#[derive(Clone, Debug)]
pub struct IoHere {
    pub remove_tabs: bool,
    pub here_end: Word,
    pub doc: Word,
}

#[derive(Clone, Debug)]
pub enum ExtendedTestExpression {
    FileExists(Word),
    FileExistsAndIsBlockSpecialFile(Word),
    FileExistsAndIsCharSpecialFile(Word),
    FileExistsAndIsDir(Word),
    FileExistsAndIsRegularFile(Word),
    FileExistsAndIsSetgid(Word),
    FileExistsAndIsSymlink(Word),
    FileExistsAndHasStickyBit(Word),
    FileExistsAndIsFifo(Word),
    FileExistsAndIsReadable(Word),
    FileExistsAndIsNotZeroLength(Word),
    FdIsOpenTerminal(Word),
    FileExistsAndIsSetuid(Word),
    FileExistsAndIsWritable(Word),
    FileExistsAndIsExecutable(Word),
    FileExistsAndOwnedByEffectiveGroupId(Word),
    FileExistsAndModifiedSinceLastRead(Word),
    FileExistsAndOwnedByEffectiveUserId(Word),
    FileExistsAndIsSocket(Word),
    FilesReferToSameDeviceAndInodeNumbers(Word, Word),
    LeftFileIsNewerOrExistsWhenRightDoesNot(Word, Word),
    LeftFileIsOlderOrDoesNotExistWhenRightDoes(Word, Word),
    ShellOptionEnabled(Word),
    ShellVariableIsSetAndAssigned(Word),
    ShellVariableIsSetAndNameRef(Word),
    StringHasZeroLength(Word),
    StringHasNonZeroLength(Word),
    StringsAreEqual(Word, Word),
    StringsNotEqual(Word, Word),
    LeftSortsBeforeRight(Word, Word),
    LeftSortsAfterRight(Word, Word),
    ArithmeticEqualTo(Word, Word),
    ArithmeticNotEqualTo(Word, Word),
    ArithmeticLessThan(Word, Word),
    ArithmeticLessThanOrEqualTo(Word, Word),
    ArithmeticGreaterThan(Word, Word),
    ArithmeticGreaterThanOrEqualTo(Word, Word),
}

#[derive(Clone, Debug)]
pub struct Word {
    pub subtokens: Vec<tokenizer::WordSubtoken>,
}

impl Word {
    pub fn from(t: &tokenizer::Token) -> Word {
        match t {
            tokenizer::Token::Word((_, subtokens), _) => Word {
                subtokens: subtokens.clone(),
            },
            tokenizer::Token::Operator(w, _) => Word {
                subtokens: vec![tokenizer::WordSubtoken::Text(w.clone())],
            },
        }
    }

    pub fn flatten(&self) -> String {
        flatten_subtokens(self.subtokens.as_ref())
    }
}

fn flatten_subtokens(subtokens: &Vec<tokenizer::WordSubtoken>) -> String {
    let mut s = String::new();
    for subtoken in subtokens {
        match subtoken {
            tokenizer::WordSubtoken::Text(t) => s.push_str(t),
            tokenizer::WordSubtoken::CommandSubstitution(cs, _) => s.push_str(cs),
            tokenizer::WordSubtoken::SingleQuotedText(t) => s.push_str(t),
            tokenizer::WordSubtoken::DoubleQuotedSequence(dq, _) => s.push_str(dq),
            tokenizer::WordSubtoken::EscapeSequence(t) => s.push_str(t),
        }
    }
    s
}
