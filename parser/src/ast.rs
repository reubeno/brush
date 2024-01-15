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
    ExtendedTest(ExtendedTestExpr),
}

#[derive(Clone, Debug)]
pub enum CompoundCommand {
    Arithmetic(ArithmeticCommand),
    ArithmeticForClause(ArithmeticForClauseCommand),
    BraceGroup(BraceGroupCommand),
    Subshell(SubshellCommand),
    ForClause(ForClauseCommand),
    CaseClause(CaseClauseCommand),
    IfClause(IfClauseCommand),
    WhileClause(WhileClauseCommand),
    UntilClause(UntilClauseCommand),
}

#[derive(Clone, Debug)]
pub struct ArithmeticCommand {
    pub expr: UnexpandedArithmeticExpr,
}

pub type SubshellCommand = CompoundList;

#[derive(Clone, Debug)]
pub struct ForClauseCommand {
    pub variable_name: String,
    pub values: Option<Vec<Word>>,
    pub body: DoGroupCommand,
}

#[derive(Clone, Debug)]
pub struct ArithmeticForClauseCommand {
    pub initializer: Option<UnexpandedArithmeticExpr>,
    pub condition: Option<UnexpandedArithmeticExpr>,
    pub updater: Option<UnexpandedArithmeticExpr>,
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
    AssignmentWord(Assignment),
}

#[derive(Clone, Debug)]
pub enum Assignment {
    Scalar { name: String, value: Word },
    Array { name: String, values: Vec<Word> },
}

pub type RedirectList = Vec<IoRedirect>;

#[derive(Clone, Debug)]
pub enum IoRedirect {
    File(Option<u32>, IoFileRedirectKind, IoFileRedirectTarget),
    HereDocument(Option<u32>, IoHereDocument),
    HereString(Option<u32>, Word),
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
    ProcessSubstitution(SubshellCommand),
}

#[derive(Clone, Debug)]
pub struct IoHereDocument {
    pub remove_tabs: bool,
    pub here_end: Word,
    pub doc: Word,
}

#[derive(Clone, Debug)]
pub enum ExtendedTestExpr {
    And(Box<ExtendedTestExpr>, Box<ExtendedTestExpr>),
    Or(Box<ExtendedTestExpr>, Box<ExtendedTestExpr>),
    Not(Box<ExtendedTestExpr>),
    Parenthesized(Box<ExtendedTestExpr>),
    UnaryTest(UnaryPredicate, Word),
    BinaryTest(BinaryPredicate, Word, Word),
}

#[derive(Clone, Debug)]
pub enum UnaryPredicate {
    FileExists,
    FileExistsAndIsBlockSpecialFile,
    FileExistsAndIsCharSpecialFile,
    FileExistsAndIsDir,
    FileExistsAndIsRegularFile,
    FileExistsAndIsSetgid,
    FileExistsAndIsSymlink,
    FileExistsAndHasStickyBit,
    FileExistsAndIsFifo,
    FileExistsAndIsReadable,
    FileExistsAndIsNotZeroLength,
    FdIsOpenTerminal,
    FileExistsAndIsSetuid,
    FileExistsAndIsWritable,
    FileExistsAndIsExecutable,
    FileExistsAndOwnedByEffectiveGroupId,
    FileExistsAndModifiedSinceLastRead,
    FileExistsAndOwnedByEffectiveUserId,
    FileExistsAndIsSocket,
    ShellOptionEnabled,
    ShellVariableIsSetAndAssigned,
    ShellVariableIsSetAndNameRef,
    StringHasZeroLength,
    StringHasNonZeroLength,
}

#[derive(Clone, Debug)]
pub enum BinaryPredicate {
    FilesReferToSameDeviceAndInodeNumbers,
    LeftFileIsNewerOrExistsWhenRightDoesNot,
    LeftFileIsOlderOrDoesNotExistWhenRightDoes,
    StringMatchesPattern,
    StringDoesNotMatchPattern,
    StringMatchesRegex,
    LeftSortsBeforeRight,
    LeftSortsAfterRight,
    ArithmeticEqualTo,
    ArithmeticNotEqualTo,
    ArithmeticLessThan,
    ArithmeticLessThanOrEqualTo,
    ArithmeticGreaterThan,
    ArithmeticGreaterThanOrEqualTo,
}

#[derive(Clone, Debug)]
pub struct Word {
    pub value: String,
}

impl Word {
    pub fn from(t: &tokenizer::Token) -> Word {
        match t {
            tokenizer::Token::Word(value, _) => Word {
                value: value.clone(),
            },
            tokenizer::Token::Operator(value, _) => Word {
                value: value.clone(),
            },
        }
    }

    pub fn flatten(&self) -> String {
        self.value.clone()
    }
}

#[derive(Clone, Debug)]
pub struct UnexpandedArithmeticExpr {
    pub value: String,
}

#[derive(Clone, Debug)]
pub enum ArithmeticExpr {
    Literal(i64),
    Reference(ArithmeticTarget),
    UnaryOp(UnaryOperator, Box<ArithmeticExpr>),
    BinaryOp(BinaryOperator, Box<ArithmeticExpr>, Box<ArithmeticExpr>),
    Conditional(
        Box<ArithmeticExpr>,
        Box<ArithmeticExpr>,
        Box<ArithmeticExpr>,
    ),
    Assignment(ArithmeticTarget, Box<ArithmeticExpr>),
    BinaryAssignment(BinaryOperator, ArithmeticTarget, Box<ArithmeticExpr>),
    UnaryAssignment(UnaryAssignmentOperator, ArithmeticTarget),
}

#[derive(Clone, Copy, Debug)]
pub enum BinaryOperator {
    Power,
    Multiply,
    Divide,
    Modulo,
    Comma,
    Add,
    Subtract,
    ShiftLeft,
    ShiftRight,
    LessThan,
    LessThanOrEqualTo,
    GreaterThan,
    GreaterThanOrEqualTo,
    Equals,
    NotEquals,
    BitwiseAnd,
    BitwiseXor,
    BitwiseOr,
    LogicalAnd,
    LogicalOr,
}

#[derive(Clone, Copy, Debug)]
pub enum UnaryOperator {
    UnaryPlus,
    UnaryMinus,
    BitwiseNot,
    LogicalNot,
}

#[derive(Clone, Copy, Debug)]
pub enum UnaryAssignmentOperator {
    PrefixIncrement,
    PrefixDecrement,
    PostfixIncrement,
    PostfixDecrement,
}

#[derive(Clone, Debug)]
pub enum ArithmeticTarget {
    Variable(String),
    ArrayElement(String, Box<ArithmeticExpr>),
}
