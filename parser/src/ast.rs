use std::fmt::{Display, Write};

use crate::tokenizer;

const DISPLAY_INDENT: &str = "    ";

/// Represents a complete shell program.
#[derive(Clone, Debug)]
pub struct Program {
    /// A sequence of complete shell commands.
    pub complete_commands: Vec<CompleteCommand>,
}

impl Display for Program {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for complete_command in &self.complete_commands {
            write!(f, "{}", complete_command)?;
        }
        Ok(())
    }
}

/// Represents a complete shell command.
pub type CompleteCommand = CompoundList;

/// Represents a complete shell command item.
pub type CompleteCommandItem = CompoundListItem;

/// Indicates whether the preceding command is executed synchronously or asynchronously.
#[derive(Clone, Debug)]
pub enum SeparatorOperator {
    /// The preceding command is executed asynchronously.
    Async,
    /// The preceding command is executed synchronously.
    Sequence,
}

impl Display for SeparatorOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SeparatorOperator::Async => write!(f, "&"),
            SeparatorOperator::Sequence => write!(f, ";"),
        }
    }
}

/// Represents a sequence of command pipelines connected by boolean operators.
#[derive(Clone, Debug)]
pub struct AndOrList {
    /// The first command pipeline.
    pub first: Pipeline,
    /// Any additional command pipelines, in sequence order.
    pub additional: Vec<AndOr>,
}

impl Display for AndOrList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.first)?;
        for item in &self.additional {
            write!(f, "{}", item)?;
        }

        Ok(())
    }
}

/// Represents a boolean operator used to connect command pipelines, along with the
/// succeeding pipeline.
#[derive(Clone, Debug)]
pub enum AndOr {
    /// Boolean AND operator; the embedded pipeline is only to be executed if the
    /// preceding command has succeeded.
    And(Pipeline),
    /// Boolean OR operator; the embedded pipeline is only to be executed if the
    /// preceding command has not succeeded.
    Or(Pipeline),
}

impl Display for AndOr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AndOr::And(pipeline) => write!(f, " && {}", pipeline),
            AndOr::Or(pipeline) => write!(f, " || {}", pipeline),
        }
    }
}

/// A pipeline of commands, where each command's output is passed as standard input
/// to the command that follows it.
#[derive(Clone, Debug)]
pub struct Pipeline {
    /// Indicates whether the result of the overall pipeline should be the logical
    /// negation of the result of the pipeline.
    pub bang: bool,
    /// The sequence of commands in the pipeline.
    pub seq: Vec<Command>,
}

impl Display for Pipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.bang {
            write!(f, "!")?;
        }
        for (i, command) in self.seq.iter().enumerate() {
            if i > 0 {
                write!(f, " |")?;
            }
            write!(f, "{}", command)?;
        }

        Ok(())
    }
}

/// Represents a shell command.
#[derive(Clone, Debug)]
pub enum Command {
    /// A simple command, directly invoking an external command, a built-in command,
    /// a shell function, or similar.
    Simple(SimpleCommand),
    /// A compound command, composed of multiple commands.
    Compound(CompoundCommand, Option<RedirectList>),
    /// A command whose side effect is to define a shell function.
    Function(FunctionDefinition),
    /// A command that evaluates an extended test expression.
    ExtendedTest(ExtendedTestExpr),
}

impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Command::Simple(simple_command) => write!(f, "{}", simple_command),
            Command::Compound(compound_command, redirect_list) => {
                write!(f, "{}", compound_command)?;
                if let Some(redirect_list) = redirect_list {
                    write!(f, "{}", redirect_list)?;
                }
                Ok(())
            }
            Command::Function(function_definition) => write!(f, "{}", function_definition),
            Command::ExtendedTest(extended_test_expr) => {
                write!(f, "[[ {} ]]", extended_test_expr)
            }
        }
    }
}

/// Represents a compound command, potentially made up of multiple nested commands.
#[derive(Clone, Debug)]
pub enum CompoundCommand {
    /// An arithmetic command, evaluating an arithmetic expression.
    Arithmetic(ArithmeticCommand),
    /// An arithmetic for clause, which loops until an arithmetic condition is reached.
    ArithmeticForClause(ArithmeticForClauseCommand),
    /// A brace group, which groups commands together.
    BraceGroup(BraceGroupCommand),
    /// A subshell, which executes commands in a subshell.
    Subshell(SubshellCommand),
    /// A for clause, which loops over a set of values.
    ForClause(ForClauseCommand),
    /// A case clause, which selects a command based on a value and a set of
    /// pattern-based filters.
    CaseClause(CaseClauseCommand),
    /// An if clause, which conditionally executes a command.
    IfClause(IfClauseCommand),
    /// A while clause, which loops while a condition is met.
    WhileClause(WhileOrUntilClauseCommand),
    /// An until clause, which loops until a condition is met.
    UntilClause(WhileOrUntilClauseCommand),
}

impl Display for CompoundCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompoundCommand::Arithmetic(arithmetic_command) => write!(f, "{}", arithmetic_command),
            CompoundCommand::ArithmeticForClause(arithmetic_for_clause_command) => {
                write!(f, "{}", arithmetic_for_clause_command)
            }
            CompoundCommand::BraceGroup(brace_group_command) => {
                write!(f, "{}", brace_group_command)
            }
            CompoundCommand::Subshell(subshell_command) => write!(f, "{}", subshell_command),
            CompoundCommand::ForClause(for_clause_command) => write!(f, "{}", for_clause_command),
            CompoundCommand::CaseClause(case_clause_command) => {
                write!(f, "{}", case_clause_command)
            }
            CompoundCommand::IfClause(if_clause_command) => write!(f, "{}", if_clause_command),
            CompoundCommand::WhileClause(while_or_until_clause_command) => {
                write!(f, "while {}", while_or_until_clause_command)
            }
            CompoundCommand::UntilClause(while_or_until_clause_command) => {
                write!(f, "until {}", while_or_until_clause_command)
            }
        }
    }
}

/// An arithmetic command.
#[derive(Clone, Debug)]
pub struct ArithmeticCommand {
    pub expr: UnexpandedArithmeticExpr,
}

impl Display for ArithmeticCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(({}))", self.expr)
    }
}

/// A command that executes an embedded command in a subshell.
#[derive(Clone, Debug)]
pub struct SubshellCommand(pub CompoundList);

impl Display for SubshellCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "( ")?;
        write!(f, "{}", self.0)?;
        write!(f, " )")
    }
}

/// Represents a command that loops over a set of values.
#[derive(Clone, Debug)]
pub struct ForClauseCommand {
    pub variable_name: String,
    pub values: Option<Vec<Word>>,
    pub body: DoGroupCommand,
}

impl Display for ForClauseCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "for {} in ", self.variable_name)?;

        if let Some(values) = &self.values {
            for (i, value) in values.iter().enumerate() {
                if i > 0 {
                    write!(f, " ")?;
                }

                write!(f, "{}", value)?;
            }
        }

        writeln!(f, ";")?;

        write!(f, "{}", self.body)
    }
}

#[derive(Clone, Debug)]
pub struct ArithmeticForClauseCommand {
    pub initializer: Option<UnexpandedArithmeticExpr>,
    pub condition: Option<UnexpandedArithmeticExpr>,
    pub updater: Option<UnexpandedArithmeticExpr>,
    pub body: DoGroupCommand,
}

impl Display for ArithmeticForClauseCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "for ((")?;

        if let Some(initializer) = &self.initializer {
            write!(f, "{}", initializer)?;
        }

        write!(f, "; ")?;

        if let Some(condition) = &self.condition {
            write!(f, "{}", condition)?;
        }

        write!(f, "; ")?;

        if let Some(updater) = &self.updater {
            write!(f, "{}", updater)?;
        }

        writeln!(f, "))")?;

        write!(f, "{}", self.body)
    }
}

#[derive(Clone, Debug)]
pub struct CaseClauseCommand {
    pub value: Word,
    pub cases: Vec<CaseItem>,
}

impl Display for CaseClauseCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "case {} in", self.value)?;
        for case in &self.cases {
            write!(indenter::indented(f).with_str(DISPLAY_INDENT), "{}", case)?;
        }
        writeln!(f)?;
        write!(f, "esac")
    }
}

#[derive(Clone, Debug)]
pub struct CompoundList(pub Vec<CompoundListItem>);

impl Display for CompoundList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, item) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }

            // Write the and-or list.
            write!(f, "{}", item.0)?;

            // Write the separator... unless we're on the list item and it's a ';'.
            if i == self.0.len() - 1 && matches!(item.1, SeparatorOperator::Sequence) {
                // Skip
            } else {
                write!(f, "{}", item.1)?;
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct CompoundListItem(pub AndOrList, pub SeparatorOperator);

impl Display for CompoundListItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)?;
        write!(f, "{}", self.1)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct IfClauseCommand {
    pub condition: CompoundList,
    pub then: CompoundList,
    pub elses: Option<Vec<ElseClause>>,
}

impl Display for IfClauseCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "if {}; then", self.condition)?;
        write!(
            indenter::indented(f).with_str(DISPLAY_INDENT),
            "{}",
            self.then
        )?;
        if let Some(elses) = &self.elses {
            for else_clause in elses {
                write!(f, "{}", else_clause)?;
            }
        }

        writeln!(f)?;
        write!(f, "fi")?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ElseClause {
    pub condition: Option<CompoundList>,
    pub body: CompoundList,
}

impl Display for ElseClause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f)?;
        if let Some(condition) = &self.condition {
            writeln!(f, "elif {}; then", condition)?;
        } else {
            writeln!(f, "else")?;
        }

        write!(
            indenter::indented(f).with_str(DISPLAY_INDENT),
            "{}",
            self.body
        )
    }
}

#[derive(Clone, Debug)]
pub struct CaseItem {
    pub patterns: Vec<Word>,
    pub cmd: Option<CompoundList>,
}

impl Display for CaseItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f)?;
        for (i, pattern) in self.patterns.iter().enumerate() {
            if i > 0 {
                write!(f, "|")?;
            }
            write!(f, "{}", pattern)?;
        }
        writeln!(f, ")")?;

        if let Some(cmd) = &self.cmd {
            write!(indenter::indented(f).with_str(DISPLAY_INDENT), "{}", cmd)?;
        }
        writeln!(f)?;
        write!(f, ";;")
    }
}

#[derive(Clone, Debug)]
pub struct WhileOrUntilClauseCommand(pub CompoundList, pub DoGroupCommand);

impl Display for WhileOrUntilClauseCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}; {}", self.0, self.1)
    }
}

#[derive(Clone, Debug)]
pub struct FunctionDefinition {
    pub fname: String,
    pub body: FunctionBody,
    pub source: String,
}

impl Display for FunctionDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} () ", self.fname)?;
        write!(f, "{}", self.body)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct FunctionBody(pub CompoundCommand, pub Option<RedirectList>);

impl Display for FunctionBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)?;
        if let Some(redirect_list) = &self.1 {
            write!(f, "{}", redirect_list)?;
        }

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct BraceGroupCommand(pub CompoundList);

impl Display for BraceGroupCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{{ ")?;
        write!(indenter::indented(f).with_str(DISPLAY_INDENT), "{}", self.0)?;
        writeln!(f)?;
        write!(f, "}}")?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct DoGroupCommand(pub CompoundList);

impl Display for DoGroupCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "do")?;
        write!(indenter::indented(f).with_str(DISPLAY_INDENT), "{}", self.0)?;
        writeln!(f)?;
        write!(f, "done")
    }
}

#[derive(Clone, Debug)]
pub struct SimpleCommand {
    pub prefix: Option<CommandPrefix>,
    pub word_or_name: Option<Word>,
    pub suffix: Option<CommandSuffix>,
}

impl Display for SimpleCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut wrote_something = false;

        if let Some(prefix) = &self.prefix {
            if wrote_something {
                write!(f, " ")?;
            }

            write!(f, "{}", prefix)?;
            wrote_something = true;
        }

        if let Some(word_or_name) = &self.word_or_name {
            if wrote_something {
                write!(f, " ")?;
            }

            write!(f, "{}", word_or_name)?;
            wrote_something = true;
        }

        if let Some(suffix) = &self.suffix {
            if wrote_something {
                write!(f, " ")?;
            }

            write!(f, "{}", suffix)?;
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct CommandPrefix(pub Vec<CommandPrefixOrSuffixItem>);

impl Display for CommandPrefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, item) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, " ")?;
            }

            write!(f, "{}", item)?;
        }
        Ok(())
    }
}

#[derive(Clone, Default, Debug)]
pub struct CommandSuffix(pub Vec<CommandPrefixOrSuffixItem>);

impl Display for CommandSuffix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, item) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, " ")?;
            }

            write!(f, "{}", item)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum CommandPrefixOrSuffixItem {
    IoRedirect(IoRedirect),
    Word(Word),
    AssignmentWord(Assignment, Word),
}

impl Display for CommandPrefixOrSuffixItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandPrefixOrSuffixItem::IoRedirect(io_redirect) => write!(f, "{}", io_redirect),
            CommandPrefixOrSuffixItem::Word(word) => write!(f, "{}", word),
            CommandPrefixOrSuffixItem::AssignmentWord(_assignment, word) => write!(f, "{}", word),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Assignment {
    pub name: AssignmentName,
    pub value: AssignmentValue,
    pub append: bool,
}

impl Display for Assignment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        if self.append {
            write!(f, "+")?;
        }
        write!(f, "={}", self.value)
    }
}

#[derive(Clone, Debug)]
pub enum AssignmentName {
    VariableName(String),
    ArrayElementName(String, String),
}

impl Display for AssignmentName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssignmentName::VariableName(name) => write!(f, "{}", name),
            AssignmentName::ArrayElementName(name, index) => {
                write!(f, "{}[{}]", name, index)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum AssignmentValue {
    Scalar(Word),
    Array(Vec<(Option<Word>, Word)>),
}

impl Display for AssignmentValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssignmentValue::Scalar(word) => write!(f, "{}", word),
            AssignmentValue::Array(words) => {
                write!(f, "(")?;
                for (i, value) in words.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    match value {
                        (Some(key), value) => write!(f, "[{}]={}", key, value)?,
                        (None, value) => write!(f, "{}", value)?,
                    }
                }
                write!(f, ")")
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct RedirectList(pub Vec<IoRedirect>);

impl Display for RedirectList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for item in &self.0 {
            write!(f, "{}", item)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum IoRedirect {
    File(Option<u32>, IoFileRedirectKind, IoFileRedirectTarget),
    HereDocument(Option<u32>, IoHereDocument),
    HereString(Option<u32>, Word),
    OutputAndError(Word),
}

impl Display for IoRedirect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IoRedirect::File(fd_num, kind, target) => {
                if let Some(fd_num) = fd_num {
                    write!(f, "{}", fd_num)?;
                }

                write!(f, "{} {}", kind, target)?;
            }
            IoRedirect::OutputAndError(target) => {
                write!(f, "&> {}", target)?;
            }
            IoRedirect::HereDocument(
                fd_num,
                IoHereDocument {
                    remove_tabs,
                    here_end,
                    doc,
                },
            ) => {
                if let Some(fd_num) = fd_num {
                    write!(f, "{}", fd_num)?;
                }

                write!(f, "<<")?;
                if *remove_tabs {
                    write!(f, "-")?;
                }

                writeln!(f, "{}", here_end)?;

                write!(f, "{}", doc)?;
                writeln!(f, "{}", here_end)?;
            }
            IoRedirect::HereString(fd_num, s) => {
                if let Some(fd_num) = fd_num {
                    write!(f, "{}", fd_num)?;
                }

                write!(f, "<<< {}", s)?;
            }
        }

        Ok(())
    }
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

impl Display for IoFileRedirectKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IoFileRedirectKind::Read => write!(f, "<"),
            IoFileRedirectKind::Write => write!(f, ">"),
            IoFileRedirectKind::Append => write!(f, ">>"),
            IoFileRedirectKind::ReadAndWrite => write!(f, "<>"),
            IoFileRedirectKind::Clobber => write!(f, ">|"),
            IoFileRedirectKind::DuplicateInput => write!(f, "<&"),
            IoFileRedirectKind::DuplicateOutput => write!(f, ">&"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum IoFileRedirectTarget {
    Filename(Word),
    Fd(u32),
    ProcessSubstitution(SubshellCommand),
}

impl Display for IoFileRedirectTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IoFileRedirectTarget::Filename(word) => write!(f, "{}", word),
            IoFileRedirectTarget::Fd(fd) => write!(f, "{}", fd),
            IoFileRedirectTarget::ProcessSubstitution(subshell_command) => {
                write!(f, "{}", subshell_command)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct IoHereDocument {
    pub remove_tabs: bool,
    pub here_end: Word,
    pub doc: Word,
}

#[derive(Clone, Debug)]
pub enum TestExpr {
    False,
    Literal(String),
    And(Box<TestExpr>, Box<TestExpr>),
    Or(Box<TestExpr>, Box<TestExpr>),
    Not(Box<TestExpr>),
    Parenthesized(Box<TestExpr>),
    UnaryTest(UnaryPredicate, String),
    BinaryTest(BinaryPredicate, String, String),
}

impl Display for TestExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestExpr::False => Ok(()),
            TestExpr::Literal(s) => write!(f, "{s}"),
            TestExpr::And(left, right) => write!(f, "{left} -a {right}"),
            TestExpr::Or(left, right) => write!(f, "{left} -o {right}"),
            TestExpr::Not(expr) => write!(f, "! {}", expr),
            TestExpr::Parenthesized(expr) => write!(f, "( {expr} )"),
            TestExpr::UnaryTest(pred, word) => write!(f, "{pred} {word}"),
            TestExpr::BinaryTest(left, op, right) => write!(f, "{left} {op} {right}"),
        }
    }
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

impl Display for ExtendedTestExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtendedTestExpr::And(left, right) => {
                write!(f, "{} && {}", left, right)
            }
            ExtendedTestExpr::Or(left, right) => {
                write!(f, "{} || {}", left, right)
            }
            ExtendedTestExpr::Not(expr) => {
                write!(f, "! {}", expr)
            }
            ExtendedTestExpr::Parenthesized(expr) => {
                write!(f, "( {} )", expr)
            }
            ExtendedTestExpr::UnaryTest(pred, word) => {
                write!(f, "{} {}", pred, word)
            }
            ExtendedTestExpr::BinaryTest(pred, left, right) => {
                write!(f, "{} {} {}", left, pred, right)
            }
        }
    }
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

impl Display for UnaryPredicate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnaryPredicate::FileExists => write!(f, "-e"),
            UnaryPredicate::FileExistsAndIsBlockSpecialFile => write!(f, "-b"),
            UnaryPredicate::FileExistsAndIsCharSpecialFile => write!(f, "-c"),
            UnaryPredicate::FileExistsAndIsDir => write!(f, "-d"),
            UnaryPredicate::FileExistsAndIsRegularFile => write!(f, "-f"),
            UnaryPredicate::FileExistsAndIsSetgid => write!(f, "-g"),
            UnaryPredicate::FileExistsAndIsSymlink => write!(f, "-h"),
            UnaryPredicate::FileExistsAndHasStickyBit => write!(f, "-k"),
            UnaryPredicate::FileExistsAndIsFifo => write!(f, "-p"),
            UnaryPredicate::FileExistsAndIsReadable => write!(f, "-r"),
            UnaryPredicate::FileExistsAndIsNotZeroLength => write!(f, "-s"),
            UnaryPredicate::FdIsOpenTerminal => write!(f, "-t"),
            UnaryPredicate::FileExistsAndIsSetuid => write!(f, "-u"),
            UnaryPredicate::FileExistsAndIsWritable => write!(f, "-w"),
            UnaryPredicate::FileExistsAndIsExecutable => write!(f, "-x"),
            UnaryPredicate::FileExistsAndOwnedByEffectiveGroupId => write!(f, "-G"),
            UnaryPredicate::FileExistsAndModifiedSinceLastRead => write!(f, "-N"),
            UnaryPredicate::FileExistsAndOwnedByEffectiveUserId => write!(f, "-O"),
            UnaryPredicate::FileExistsAndIsSocket => write!(f, "-S"),
            UnaryPredicate::ShellOptionEnabled => write!(f, "-o"),
            UnaryPredicate::ShellVariableIsSetAndAssigned => write!(f, "-v"),
            UnaryPredicate::ShellVariableIsSetAndNameRef => write!(f, "-R"),
            UnaryPredicate::StringHasZeroLength => write!(f, "-z"),
            UnaryPredicate::StringHasNonZeroLength => write!(f, "-n"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum BinaryPredicate {
    FilesReferToSameDeviceAndInodeNumbers,
    LeftFileIsNewerOrExistsWhenRightDoesNot,
    LeftFileIsOlderOrDoesNotExistWhenRightDoes,
    StringExactlyMatchesPattern,
    StringDoesNotExactlyMatchPattern,
    StringMatchesRegex,
    StringContainsSubstring,
    LeftSortsBeforeRight,
    LeftSortsAfterRight,
    ArithmeticEqualTo,
    ArithmeticNotEqualTo,
    ArithmeticLessThan,
    ArithmeticLessThanOrEqualTo,
    ArithmeticGreaterThan,
    ArithmeticGreaterThanOrEqualTo,
}

impl Display for BinaryPredicate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinaryPredicate::FilesReferToSameDeviceAndInodeNumbers => write!(f, "-ef"),
            BinaryPredicate::LeftFileIsNewerOrExistsWhenRightDoesNot => write!(f, "-nt"),
            BinaryPredicate::LeftFileIsOlderOrDoesNotExistWhenRightDoes => write!(f, "-ot"),
            BinaryPredicate::StringExactlyMatchesPattern => write!(f, "=="),
            BinaryPredicate::StringDoesNotExactlyMatchPattern => write!(f, "!="),
            BinaryPredicate::StringMatchesRegex => write!(f, "=~"),
            BinaryPredicate::StringContainsSubstring => write!(f, "=~"),
            BinaryPredicate::LeftSortsBeforeRight => write!(f, "<"),
            BinaryPredicate::LeftSortsAfterRight => write!(f, ">"),
            BinaryPredicate::ArithmeticEqualTo => write!(f, "-eq"),
            BinaryPredicate::ArithmeticNotEqualTo => write!(f, "-ne"),
            BinaryPredicate::ArithmeticLessThan => write!(f, "-lt"),
            BinaryPredicate::ArithmeticLessThanOrEqualTo => write!(f, "-le"),
            BinaryPredicate::ArithmeticGreaterThan => write!(f, "-gt"),
            BinaryPredicate::ArithmeticGreaterThanOrEqualTo => write!(f, "-ge"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Word {
    pub value: String,
}

impl Display for Word {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl From<&tokenizer::Token> for Word {
    fn from(t: &tokenizer::Token) -> Word {
        match t {
            tokenizer::Token::Word(value, _) => Word {
                value: value.clone(),
            },
            tokenizer::Token::Operator(value, _) => Word {
                value: value.clone(),
            },
        }
    }
}

impl Word {
    pub fn new(s: &str) -> Self {
        Self {
            value: s.to_owned(),
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

impl Display for UnexpandedArithmeticExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
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

impl Display for ArithmeticExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArithmeticExpr::Literal(literal) => write!(f, "{literal}"),
            ArithmeticExpr::Reference(target) => write!(f, "{target}"),
            ArithmeticExpr::UnaryOp(op, operand) => write!(f, "{op}{operand}"),
            ArithmeticExpr::BinaryOp(op, left, right) => {
                if matches!(op, BinaryOperator::Comma) {
                    write!(f, "{left}{op} {right}")
                } else {
                    write!(f, "{left} {op} {right}")
                }
            }
            ArithmeticExpr::Conditional(condition, if_branch, else_branch) => {
                write!(f, "{condition} ? {if_branch} : {else_branch}")
            }
            ArithmeticExpr::Assignment(target, value) => write!(f, "{target} = {value}"),
            ArithmeticExpr::BinaryAssignment(op, target, operand) => {
                write!(f, "{target} {op}= {operand}")
            }
            ArithmeticExpr::UnaryAssignment(op, target) => match op {
                UnaryAssignmentOperator::PrefixIncrement
                | UnaryAssignmentOperator::PrefixDecrement => write!(f, "{op}{target}"),
                UnaryAssignmentOperator::PostfixIncrement
                | UnaryAssignmentOperator::PostfixDecrement => write!(f, "{target}{op}"),
            },
        }
    }
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

impl Display for BinaryOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinaryOperator::Power => write!(f, "**"),
            BinaryOperator::Multiply => write!(f, "*"),
            BinaryOperator::Divide => write!(f, "/"),
            BinaryOperator::Modulo => write!(f, "%"),
            BinaryOperator::Comma => write!(f, ","),
            BinaryOperator::Add => write!(f, "+"),
            BinaryOperator::Subtract => write!(f, "-"),
            BinaryOperator::ShiftLeft => write!(f, "<<"),
            BinaryOperator::ShiftRight => write!(f, ">>"),
            BinaryOperator::LessThan => write!(f, "<"),
            BinaryOperator::LessThanOrEqualTo => write!(f, "<="),
            BinaryOperator::GreaterThan => write!(f, ">"),
            BinaryOperator::GreaterThanOrEqualTo => write!(f, ">="),
            BinaryOperator::Equals => write!(f, "=="),
            BinaryOperator::NotEquals => write!(f, "!="),
            BinaryOperator::BitwiseAnd => write!(f, "&"),
            BinaryOperator::BitwiseXor => write!(f, "^"),
            BinaryOperator::BitwiseOr => write!(f, "|"),
            BinaryOperator::LogicalAnd => write!(f, "&&"),
            BinaryOperator::LogicalOr => write!(f, "||"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum UnaryOperator {
    UnaryPlus,
    UnaryMinus,
    BitwiseNot,
    LogicalNot,
}

impl Display for UnaryOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnaryOperator::UnaryPlus => write!(f, "+"),
            UnaryOperator::UnaryMinus => write!(f, "-"),
            UnaryOperator::BitwiseNot => write!(f, "~"),
            UnaryOperator::LogicalNot => write!(f, "!"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum UnaryAssignmentOperator {
    PrefixIncrement,
    PrefixDecrement,
    PostfixIncrement,
    PostfixDecrement,
}

impl Display for UnaryAssignmentOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnaryAssignmentOperator::PrefixIncrement => write!(f, "++"),
            UnaryAssignmentOperator::PrefixDecrement => write!(f, "--"),
            UnaryAssignmentOperator::PostfixIncrement => write!(f, "++"),
            UnaryAssignmentOperator::PostfixDecrement => write!(f, "--"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum ArithmeticTarget {
    Variable(String),
    ArrayElement(String, Box<ArithmeticExpr>),
}

impl Display for ArithmeticTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArithmeticTarget::Variable(name) => write!(f, "{name}"),
            ArithmeticTarget::ArrayElement(name, index) => write!(f, "{}[{}]", name, index),
        }
    }
}
