//! Defines the Abstract Syntax Tree (ast) for shell programs. Includes types and utilities
//! for manipulating the AST.

use std::fmt::{Display, Write};

use crate::tokenizer;

const DISPLAY_INDENT: &str = "    ";

/// Represents a complete shell program.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub struct Program {
    /// A sequence of complete shell commands.
    #[cfg_attr(test, serde(rename = "cmds"))]
    pub complete_commands: Vec<CompleteCommand>,
}

impl Display for Program {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for complete_command in &self.complete_commands {
            write!(f, "{complete_command}")?;
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
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
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
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
#[cfg_attr(test, serde(rename = "AndOr"))]
pub struct AndOrList {
    /// The first command pipeline.
    pub first: Pipeline,
    /// Any additional command pipelines, in sequence order.
    #[cfg_attr(test, serde(skip_serializing_if = "Vec::is_empty"))]
    pub additional: Vec<AndOr>,
}

impl Display for AndOrList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.first)?;
        for item in &self.additional {
            write!(f, "{item}")?;
        }

        Ok(())
    }
}

/// Represents a boolean operator used to connect command pipelines in an [`AndOrList`]
#[derive(PartialEq, Eq)]
pub enum PipelineOperator {
    /// The command pipelines are connected by a boolean AND operator.
    And,
    /// The command pipelines are connected by a boolean OR operator.
    Or,
}

impl PartialEq<AndOr> for PipelineOperator {
    fn eq(&self, other: &AndOr) -> bool {
        matches!(
            (self, other),
            (PipelineOperator::And, AndOr::And(_)) | (PipelineOperator::Or, AndOr::Or(_))
        )
    }
}

// We cannot losslessly convert into `AndOr`, hence we can only do `Into`.
#[allow(clippy::from_over_into)]
impl Into<PipelineOperator> for AndOr {
    fn into(self) -> PipelineOperator {
        match self {
            AndOr::And(_) => PipelineOperator::And,
            AndOr::Or(_) => PipelineOperator::Or,
        }
    }
}

/// An iterator over the pipelines in an [`AndOrList`].
pub struct AndOrListIter<'a> {
    first: Option<&'a Pipeline>,
    additional_iter: std::slice::Iter<'a, AndOr>,
}

impl<'a> Iterator for AndOrListIter<'a> {
    type Item = (PipelineOperator, &'a Pipeline);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(first) = self.first.take() {
            Some((PipelineOperator::And, first))
        } else {
            self.additional_iter.next().map(|and_or| match and_or {
                AndOr::And(pipeline) => (PipelineOperator::And, pipeline),
                AndOr::Or(pipeline) => (PipelineOperator::Or, pipeline),
            })
        }
    }
}

impl<'a> IntoIterator for &'a AndOrList {
    type Item = (PipelineOperator, &'a Pipeline);
    type IntoIter = AndOrListIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        AndOrListIter {
            first: Some(&self.first),
            additional_iter: self.additional.iter(),
        }
    }
}

impl<'a> From<(PipelineOperator, &'a Pipeline)> for AndOr {
    fn from(value: (PipelineOperator, &'a Pipeline)) -> Self {
        match value.0 {
            PipelineOperator::Or => Self::Or(value.1.to_owned()),
            PipelineOperator::And => Self::And(value.1.to_owned()),
        }
    }
}

impl AndOrList {
    /// Returns an iterator over the pipelines in this `AndOrList`.
    pub fn iter(&self) -> AndOrListIter<'_> {
        self.into_iter()
    }
}

/// Represents a boolean operator used to connect command pipelines, along with the
/// succeeding pipeline.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
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
            AndOr::And(pipeline) => write!(f, " && {pipeline}"),
            AndOr::Or(pipeline) => write!(f, " || {pipeline}"),
        }
    }
}

/// The type of timing requested for a pipeline.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum PipelineTimed {
    /// The pipeline should be timed with bash-like output.
    Timed,
    /// The pipeline should be timed with POSIX-like output.
    TimedWithPosixOutput,
}

/// A pipeline of commands, where each command's output is passed as standard input
/// to the command that follows it.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub struct Pipeline {
    /// Indicates whether the pipeline's execution should be timed with reported
    /// timings in output.
    #[cfg_attr(test, serde(skip_serializing_if = "Option::is_none"))]
    pub timed: Option<PipelineTimed>,
    /// Indicates whether the result of the overall pipeline should be the logical
    /// negation of the result of the pipeline.
    #[cfg_attr(test, serde(skip_serializing_if = "<&bool as std::ops::Not>::not"))]
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
            write!(f, "{command}")?;
        }

        Ok(())
    }
}

/// Represents a shell command.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
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
            Command::Simple(simple_command) => write!(f, "{simple_command}"),
            Command::Compound(compound_command, redirect_list) => {
                write!(f, "{compound_command}")?;
                if let Some(redirect_list) = redirect_list {
                    write!(f, "{redirect_list}")?;
                }
                Ok(())
            }
            Command::Function(function_definition) => write!(f, "{function_definition}"),
            Command::ExtendedTest(extended_test_expr) => {
                write!(f, "[[ {extended_test_expr} ]]")
            }
        }
    }
}

/// Represents a compound command, potentially made up of multiple nested commands.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
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
            CompoundCommand::Arithmetic(arithmetic_command) => write!(f, "{arithmetic_command}"),
            CompoundCommand::ArithmeticForClause(arithmetic_for_clause_command) => {
                write!(f, "{arithmetic_for_clause_command}")
            }
            CompoundCommand::BraceGroup(brace_group_command) => {
                write!(f, "{brace_group_command}")
            }
            CompoundCommand::Subshell(subshell_command) => write!(f, "{subshell_command}"),
            CompoundCommand::ForClause(for_clause_command) => write!(f, "{for_clause_command}"),
            CompoundCommand::CaseClause(case_clause_command) => {
                write!(f, "{case_clause_command}")
            }
            CompoundCommand::IfClause(if_clause_command) => write!(f, "{if_clause_command}"),
            CompoundCommand::WhileClause(while_or_until_clause_command) => {
                write!(f, "while {while_or_until_clause_command}")
            }
            CompoundCommand::UntilClause(while_or_until_clause_command) => {
                write!(f, "until {while_or_until_clause_command}")
            }
        }
    }
}

/// An arithmetic command, evaluating an arithmetic expression.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub struct ArithmeticCommand {
    /// The raw, unparsed and unexpanded arithmetic expression.
    pub expr: UnexpandedArithmeticExpr,
}

impl Display for ArithmeticCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(({}))", self.expr)
    }
}

/// A subshell, which executes commands in a subshell.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub struct SubshellCommand(pub CompoundList);

impl Display for SubshellCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "( ")?;
        write!(f, "{}", self.0)?;
        write!(f, " )")
    }
}

/// A for clause, which loops over a set of values.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub struct ForClauseCommand {
    /// The name of the iterator variable.
    pub variable_name: String,
    /// The values being iterated over.
    pub values: Option<Vec<Word>>,
    /// The command to run for each iteration of the loop.
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

                write!(f, "{value}")?;
            }
        }

        writeln!(f, ";")?;

        write!(f, "{}", self.body)
    }
}

/// An arithmetic for clause, which loops until an arithmetic condition is reached.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub struct ArithmeticForClauseCommand {
    /// Optionally, the initializer expression evaluated before the first iteration of the loop.
    pub initializer: Option<UnexpandedArithmeticExpr>,
    /// Optionally, the expression evaluated as the exit condition of the loop.
    pub condition: Option<UnexpandedArithmeticExpr>,
    /// Optionally, the expression evaluated after each iteration of the loop.
    pub updater: Option<UnexpandedArithmeticExpr>,
    /// The command to run for each iteration of the loop.
    pub body: DoGroupCommand,
}

impl Display for ArithmeticForClauseCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "for ((")?;

        if let Some(initializer) = &self.initializer {
            write!(f, "{initializer}")?;
        }

        write!(f, "; ")?;

        if let Some(condition) = &self.condition {
            write!(f, "{condition}")?;
        }

        write!(f, "; ")?;

        if let Some(updater) = &self.updater {
            write!(f, "{updater}")?;
        }

        writeln!(f, "))")?;

        write!(f, "{}", self.body)
    }
}

/// A case clause, which selects a command based on a value and a set of
/// pattern-based filters.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub struct CaseClauseCommand {
    /// The value being matched on.
    pub value: Word,
    /// The individual case branches.
    pub cases: Vec<CaseItem>,
}

impl Display for CaseClauseCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "case {} in", self.value)?;
        for case in &self.cases {
            write!(indenter::indented(f).with_str(DISPLAY_INDENT), "{case}")?;
        }
        writeln!(f)?;
        write!(f, "esac")
    }
}

/// A sequence of commands.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
#[cfg_attr(test, serde(rename = "List"))]
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

/// An element of a compound command list.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
#[cfg_attr(test, serde(rename = "Item"))]
pub struct CompoundListItem(pub AndOrList, pub SeparatorOperator);

impl Display for CompoundListItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)?;
        write!(f, "{}", self.1)?;
        Ok(())
    }
}

/// An if clause, which conditionally executes a command.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub struct IfClauseCommand {
    /// The command whose execution result is inspected.
    pub condition: CompoundList,
    /// The command to execute if the condition is true.
    pub then: CompoundList,
    /// Optionally, `else` clauses that will be evaluated if the condition is false.
    #[cfg_attr(test, serde(skip_serializing_if = "Option::is_none"))]
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
                write!(f, "{else_clause}")?;
            }
        }

        writeln!(f)?;
        write!(f, "fi")?;

        Ok(())
    }
}

/// Represents the `else` clause of a conditional command.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub struct ElseClause {
    /// If present, the condition that must be met for this `else` clause to be executed.
    #[cfg_attr(test, serde(skip_serializing_if = "Option::is_none"))]
    pub condition: Option<CompoundList>,
    /// The commands to execute if this `else` clause is selected.
    pub body: CompoundList,
}

impl Display for ElseClause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f)?;
        if let Some(condition) = &self.condition {
            writeln!(f, "elif {condition}; then")?;
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

/// An individual matching case item in a case clause.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub struct CaseItem {
    /// The patterns that select this case branch.
    pub patterns: Vec<Word>,
    /// The commands to execute if this case branch is selected.
    pub cmd: Option<CompoundList>,
    /// When the case branch is selected, the action to take after the command is executed.
    pub post_action: CaseItemPostAction,
}

impl Display for CaseItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f)?;
        for (i, pattern) in self.patterns.iter().enumerate() {
            if i > 0 {
                write!(f, "|")?;
            }
            write!(f, "{pattern}")?;
        }
        writeln!(f, ")")?;

        if let Some(cmd) = &self.cmd {
            write!(indenter::indented(f).with_str(DISPLAY_INDENT), "{cmd}")?;
        }
        writeln!(f)?;
        write!(f, "{}", self.post_action)
    }
}

/// Describes the action to take after executing the body command of a case clause.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum CaseItemPostAction {
    /// The containing case should be exited.
    ExitCase,
    /// If one is present, the command body of the succeeding case item should be
    /// executed (without evaluating its pattern).
    UnconditionallyExecuteNextCaseItem,
    /// The case should continue evaluating the remaining case items, as if this
    /// item had not been executed.
    ContinueEvaluatingCases,
}

impl Display for CaseItemPostAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaseItemPostAction::ExitCase => write!(f, ";;"),
            CaseItemPostAction::UnconditionallyExecuteNextCaseItem => write!(f, ";&"),
            CaseItemPostAction::ContinueEvaluatingCases => write!(f, ";;&"),
        }
    }
}

/// A while or until clause, whose looping is controlled by a condition.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub struct WhileOrUntilClauseCommand(pub CompoundList, pub DoGroupCommand);

impl Display for WhileOrUntilClauseCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}; {}", self.0, self.1)
    }
}

/// Encapsulates the definition of a shell function.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub struct FunctionDefinition {
    /// The name of the function.
    pub fname: String,
    /// The body of the function.
    pub body: FunctionBody,
    /// The source of the function definition.
    pub source: String,
}

impl Display for FunctionDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} () ", self.fname)?;
        write!(f, "{}", self.body)?;
        Ok(())
    }
}

/// Encapsulates the body of a function definition.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub struct FunctionBody(pub CompoundCommand, pub Option<RedirectList>);

impl Display for FunctionBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)?;
        if let Some(redirect_list) = &self.1 {
            write!(f, "{redirect_list}")?;
        }

        Ok(())
    }
}

/// A brace group, which groups commands together.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
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

/// A do group, which groups commands together.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub struct DoGroupCommand(pub CompoundList);

impl Display for DoGroupCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "do")?;
        write!(indenter::indented(f).with_str(DISPLAY_INDENT), "{}", self.0)?;
        writeln!(f)?;
        write!(f, "done")
    }
}

/// Represents the invocation of a simple command.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
#[cfg_attr(test, serde(rename = "Simple"))]
pub struct SimpleCommand {
    /// Optionally, a prefix to the command.
    #[cfg_attr(test, serde(skip_serializing_if = "Option::is_none"))]
    pub prefix: Option<CommandPrefix>,
    /// The name of the command to execute.
    #[cfg_attr(test, serde(skip_serializing_if = "Option::is_none"))]
    #[cfg_attr(test, serde(rename = "w"))]
    pub word_or_name: Option<Word>,
    /// Optionally, a suffix to the command.
    #[cfg_attr(test, serde(skip_serializing_if = "Option::is_none"))]
    pub suffix: Option<CommandSuffix>,
}

impl Display for SimpleCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut wrote_something = false;

        if let Some(prefix) = &self.prefix {
            if wrote_something {
                write!(f, " ")?;
            }

            write!(f, "{prefix}")?;
            wrote_something = true;
        }

        if let Some(word_or_name) = &self.word_or_name {
            if wrote_something {
                write!(f, " ")?;
            }

            write!(f, "{word_or_name}")?;
            wrote_something = true;
        }

        if let Some(suffix) = &self.suffix {
            if wrote_something {
                write!(f, " ")?;
            }

            write!(f, "{suffix}")?;
        }

        Ok(())
    }
}

/// Represents a prefix to a simple command.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
#[cfg_attr(test, serde(rename = "Prefix"))]
pub struct CommandPrefix(pub Vec<CommandPrefixOrSuffixItem>);

impl Display for CommandPrefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, item) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, " ")?;
            }

            write!(f, "{item}")?;
        }
        Ok(())
    }
}

/// Represents a suffix to a simple command; a word argument, declaration, or I/O redirection.
#[derive(Clone, Default, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
#[cfg_attr(test, serde(rename = "Suffix"))]
pub struct CommandSuffix(pub Vec<CommandPrefixOrSuffixItem>);

impl Display for CommandSuffix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, item) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, " ")?;
            }

            write!(f, "{item}")?;
        }
        Ok(())
    }
}

/// Represents the I/O direction of a process substitution.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum ProcessSubstitutionKind {
    /// The process is read from.
    Read,
    /// The process is written to.
    Write,
}

impl Display for ProcessSubstitutionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessSubstitutionKind::Read => write!(f, "<"),
            ProcessSubstitutionKind::Write => write!(f, ">"),
        }
    }
}

/// A prefix or suffix for a simple command.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum CommandPrefixOrSuffixItem {
    /// An I/O redirection.
    IoRedirect(IoRedirect),
    /// A word.
    Word(Word),
    /// An assignment/declaration word.
    #[cfg_attr(test, serde(rename = "Assign"))]
    AssignmentWord(Assignment, Word),
    /// A process substitution.
    ProcessSubstitution(ProcessSubstitutionKind, SubshellCommand),
}

impl Display for CommandPrefixOrSuffixItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandPrefixOrSuffixItem::IoRedirect(io_redirect) => write!(f, "{io_redirect}"),
            CommandPrefixOrSuffixItem::Word(word) => write!(f, "{word}"),
            CommandPrefixOrSuffixItem::AssignmentWord(_assignment, word) => write!(f, "{word}"),
            CommandPrefixOrSuffixItem::ProcessSubstitution(kind, subshell_command) => {
                write!(f, "{kind}({subshell_command})")
            }
        }
    }
}

/// Encapsulates an assignment declaration.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
#[cfg_attr(test, serde(rename = "Assign"))]
pub struct Assignment {
    /// Name being assigned to.
    pub name: AssignmentName,
    /// Value being assigned.
    pub value: AssignmentValue,
    /// Whether or not to append to the preexisting value associated with the named variable.
    #[cfg_attr(test, serde(skip_serializing_if = "<&bool as std::ops::Not>::not"))]
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

/// The target of an assignment.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum AssignmentName {
    /// A named variable.
    #[cfg_attr(test, serde(rename = "Var"))]
    VariableName(String),
    /// An element in a named array.
    ArrayElementName(String, String),
}

impl Display for AssignmentName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssignmentName::VariableName(name) => write!(f, "{name}"),
            AssignmentName::ArrayElementName(name, index) => {
                write!(f, "{name}[{index}]")
            }
        }
    }
}

/// A value being assigned to a variable.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum AssignmentValue {
    /// A scalar (word) value.
    Scalar(Word),
    /// An array of elements.
    Array(Vec<(Option<Word>, Word)>),
}

impl Display for AssignmentValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssignmentValue::Scalar(word) => write!(f, "{word}"),
            AssignmentValue::Array(words) => {
                write!(f, "(")?;
                for (i, value) in words.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    match value {
                        (Some(key), value) => write!(f, "[{key}]={value}")?,
                        (None, value) => write!(f, "{value}")?,
                    }
                }
                write!(f, ")")
            }
        }
    }
}

/// A list of I/O redirections to be applied to a command.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub struct RedirectList(pub Vec<IoRedirect>);

impl Display for RedirectList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for item in &self.0 {
            write!(f, "{item}")?;
        }
        Ok(())
    }
}

/// An I/O redirection.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum IoRedirect {
    /// Redirection to a file.
    File(Option<u32>, IoFileRedirectKind, IoFileRedirectTarget),
    /// Redirection from a here-document.
    HereDocument(Option<u32>, IoHereDocument),
    /// Redirection from a here-string.
    HereString(Option<u32>, Word),
    /// Redirection of both standard output and standard error (with optional append).
    OutputAndError(Word, bool),
}

impl Display for IoRedirect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IoRedirect::File(fd_num, kind, target) => {
                if let Some(fd_num) = fd_num {
                    write!(f, "{fd_num}")?;
                }

                write!(f, "{kind} {target}")?;
            }
            IoRedirect::OutputAndError(target, append) => {
                write!(f, "&>")?;
                if *append {
                    write!(f, ">")?;
                }
                write!(f, " {target}")?;
            }
            IoRedirect::HereDocument(
                fd_num,
                IoHereDocument {
                    remove_tabs,
                    here_end,
                    doc,
                    ..
                },
            ) => {
                if let Some(fd_num) = fd_num {
                    write!(f, "{fd_num}")?;
                }

                write!(f, "<<")?;
                if *remove_tabs {
                    write!(f, "-")?;
                }

                writeln!(f, "{here_end}")?;

                write!(f, "{doc}")?;
                writeln!(f, "{here_end}")?;
            }
            IoRedirect::HereString(fd_num, s) => {
                if let Some(fd_num) = fd_num {
                    write!(f, "{fd_num}")?;
                }

                write!(f, "<<< {s}")?;
            }
        }

        Ok(())
    }
}

/// Kind of file I/O redirection.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum IoFileRedirectKind {
    /// Read (`<`).
    Read,
    /// Write (`>`).
    Write,
    /// Append (`>>`).
    Append,
    /// Read and write (`<>`).
    ReadAndWrite,
    /// Clobber (`>|`).
    Clobber,
    /// Duplicate input (`<&`).
    DuplicateInput,
    /// Duplicate output (`>&`).
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

/// Target for an I/O file redirection.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum IoFileRedirectTarget {
    /// Path to a file.
    Filename(Word),
    /// File descriptor number.
    Fd(u32),
    /// Process substitution: substitution with the results of executing the given
    /// command in a subshell.
    ProcessSubstitution(ProcessSubstitutionKind, SubshellCommand),
}

impl Display for IoFileRedirectTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IoFileRedirectTarget::Filename(word) => write!(f, "{word}"),
            IoFileRedirectTarget::Fd(fd) => write!(f, "{fd}"),
            IoFileRedirectTarget::ProcessSubstitution(kind, subshell_command) => {
                write!(f, "{kind}{subshell_command}")
            }
        }
    }
}

/// Represents an I/O here document.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub struct IoHereDocument {
    /// Whether to remove leading tabs from the here document.
    #[cfg_attr(test, serde(skip_serializing_if = "<&bool as std::ops::Not>::not"))]
    pub remove_tabs: bool,
    /// Whether to basic-expand the contents of the here document.
    #[cfg_attr(test, serde(skip_serializing_if = "<&bool as std::ops::Not>::not"))]
    pub requires_expansion: bool,
    /// The delimiter marking the end of the here document.
    pub here_end: Word,
    /// The contents of the here document.
    pub doc: Word,
}

/// A (non-extended) test expression.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum TestExpr {
    /// Always evaluates to false.
    False,
    /// A literal string.
    Literal(String),
    /// Logical AND operation on two nested expressions.
    And(Box<TestExpr>, Box<TestExpr>),
    /// Logical OR operation on two nested expressions.
    Or(Box<TestExpr>, Box<TestExpr>),
    /// Logical NOT operation on a nested expression.
    Not(Box<TestExpr>),
    /// A parenthesized expression.
    Parenthesized(Box<TestExpr>),
    /// A unary test operation.
    UnaryTest(UnaryPredicate, String),
    /// A binary test operation.
    BinaryTest(BinaryPredicate, String, String),
}

impl Display for TestExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestExpr::False => Ok(()),
            TestExpr::Literal(s) => write!(f, "{s}"),
            TestExpr::And(left, right) => write!(f, "{left} -a {right}"),
            TestExpr::Or(left, right) => write!(f, "{left} -o {right}"),
            TestExpr::Not(expr) => write!(f, "! {expr}"),
            TestExpr::Parenthesized(expr) => write!(f, "( {expr} )"),
            TestExpr::UnaryTest(pred, word) => write!(f, "{pred} {word}"),
            TestExpr::BinaryTest(left, op, right) => write!(f, "{left} {op} {right}"),
        }
    }
}

/// An extended test expression.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum ExtendedTestExpr {
    /// Logical AND operation on two nested expressions.
    And(Box<ExtendedTestExpr>, Box<ExtendedTestExpr>),
    /// Logical OR operation on two nested expressions.
    Or(Box<ExtendedTestExpr>, Box<ExtendedTestExpr>),
    /// Logical NOT operation on a nested expression.
    Not(Box<ExtendedTestExpr>),
    /// A parenthesized expression.
    Parenthesized(Box<ExtendedTestExpr>),
    /// A unary test operation.
    UnaryTest(UnaryPredicate, Word),
    /// A binary test operation.
    BinaryTest(BinaryPredicate, Word, Word),
}

impl Display for ExtendedTestExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtendedTestExpr::And(left, right) => {
                write!(f, "{left} && {right}")
            }
            ExtendedTestExpr::Or(left, right) => {
                write!(f, "{left} || {right}")
            }
            ExtendedTestExpr::Not(expr) => {
                write!(f, "! {expr}")
            }
            ExtendedTestExpr::Parenthesized(expr) => {
                write!(f, "( {expr} )")
            }
            ExtendedTestExpr::UnaryTest(pred, word) => {
                write!(f, "{pred} {word}")
            }
            ExtendedTestExpr::BinaryTest(pred, left, right) => {
                write!(f, "{left} {pred} {right}")
            }
        }
    }
}

/// A unary predicate usable in an extended test expression.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum UnaryPredicate {
    /// Computes if the operand is a path to an existing file.
    FileExists,
    /// Computes if the operand is a path to an existing block device file.
    FileExistsAndIsBlockSpecialFile,
    /// Computes if the operand is a path to an existing character device file.
    FileExistsAndIsCharSpecialFile,
    /// Computes if the operand is a path to an existing directory.
    FileExistsAndIsDir,
    /// Computes if the operand is a path to an existing regular file.
    FileExistsAndIsRegularFile,
    /// Computes if the operand is a path to an existing file with the setgid bit set.
    FileExistsAndIsSetgid,
    /// Computes if the operand is a path to an existing symbolic link.
    FileExistsAndIsSymlink,
    /// Computes if the operand is a path to an existing file with the sticky bit set.
    FileExistsAndHasStickyBit,
    /// Computes if the operand is a path to an existing FIFO file.
    FileExistsAndIsFifo,
    /// Computes if the operand is a path to an existing file that is readable.
    FileExistsAndIsReadable,
    /// Computes if the operand is a path to an existing file with a non-zero length.
    FileExistsAndIsNotZeroLength,
    /// Computes if the operand is a file descriptor that is an open terminal.
    FdIsOpenTerminal,
    /// Computes if the operand is a path to an existing file with the setuid bit set.
    FileExistsAndIsSetuid,
    /// Computes if the operand is a path to an existing file that is writable.
    FileExistsAndIsWritable,
    /// Computes if the operand is a path to an existing file that is executable.
    FileExistsAndIsExecutable,
    /// Computes if the operand is a path to an existing file owned by the current context's
    /// effective group ID.
    FileExistsAndOwnedByEffectiveGroupId,
    /// Computes if the operand is a path to an existing file that has been modified since last
    /// being read.
    FileExistsAndModifiedSinceLastRead,
    /// Computes if the operand is a path to an existing file owned by the current context's
    /// effective user ID.
    FileExistsAndOwnedByEffectiveUserId,
    /// Computes if the operand is a path to an existing socket file.
    FileExistsAndIsSocket,
    /// Computes if the operand is a 'set -o' option that is enabled.
    ShellOptionEnabled,
    /// Computes if the operand names a shell variable that is set and assigned a value.
    ShellVariableIsSetAndAssigned,
    /// Computes if the operand names a shell variable that is set and of nameref type.
    ShellVariableIsSetAndNameRef,
    /// Computes if the operand is a string with zero length.
    StringHasZeroLength,
    /// Computes if the operand is a string with non-zero length.
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

/// A binary predicate usable in an extended test expression.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum BinaryPredicate {
    /// Computes if two files refer to the same device and inode numbers.
    FilesReferToSameDeviceAndInodeNumbers,
    /// Computes if the left file is newer than the right, or exists when the right does not.
    LeftFileIsNewerOrExistsWhenRightDoesNot,
    /// Computes if the left file is older than the right, or does not exist when the right does.
    LeftFileIsOlderOrDoesNotExistWhenRightDoes,
    /// Computes if a string exactly matches a pattern.
    StringExactlyMatchesPattern,
    /// Computes if a string does not exactly match a pattern.
    StringDoesNotExactlyMatchPattern,
    /// Computes if a string matches a regular expression.
    StringMatchesRegex,
    /// Computes if a string exactly matches another string.
    StringExactlyMatchesString,
    /// Computes if a string does not exactly match another string.
    StringDoesNotExactlyMatchString,
    /// Computes if a string contains a substring.
    StringContainsSubstring,
    /// Computes if the left value sorts before the right.
    LeftSortsBeforeRight,
    /// Computes if the left value sorts after the right.
    LeftSortsAfterRight,
    /// Computes if two values are equal via arithmetic comparison.
    ArithmeticEqualTo,
    /// Computes if two values are not equal via arithmetic comparison.
    ArithmeticNotEqualTo,
    /// Computes if the left value is less than the right via arithmetic comparison.
    ArithmeticLessThan,
    /// Computes if the left value is less than or equal to the right via arithmetic comparison.
    ArithmeticLessThanOrEqualTo,
    /// Computes if the left value is greater than the right via arithmetic comparison.
    ArithmeticGreaterThan,
    /// Computes if the left value is greater than or equal to the right via arithmetic comparison.
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
            BinaryPredicate::StringExactlyMatchesString => write!(f, "=="),
            BinaryPredicate::StringDoesNotExactlyMatchString => write!(f, "!="),
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

/// Represents a shell word.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
#[cfg_attr(test, serde(rename = "W"))]
pub struct Word {
    /// Raw text of the word.
    #[cfg_attr(test, serde(rename = "v"))]
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

impl From<String> for Word {
    fn from(s: String) -> Word {
        Word { value: s }
    }
}

impl Word {
    /// Constructs a new `Word` from a given string.
    pub fn new(s: &str) -> Self {
        Self {
            value: s.to_owned(),
        }
    }

    /// Returns the raw text of the word, consuming the `Word`.
    pub fn flatten(&self) -> String {
        self.value.clone()
    }
}

/// Encapsulates an unparsed arithmetic expression.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub struct UnexpandedArithmeticExpr {
    /// The raw text of the expression.
    pub value: String,
}

impl Display for UnexpandedArithmeticExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

/// An arithmetic expression.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum ArithmeticExpr {
    /// A literal integer value.
    Literal(i64),
    /// A dereference of a variable or array element.
    Reference(ArithmeticTarget),
    /// A unary operation on an the result of a given nested expression.
    UnaryOp(UnaryOperator, Box<ArithmeticExpr>),
    /// A binary operation on two nested expressions.
    BinaryOp(BinaryOperator, Box<ArithmeticExpr>, Box<ArithmeticExpr>),
    /// A ternary conditional expression.
    Conditional(
        Box<ArithmeticExpr>,
        Box<ArithmeticExpr>,
        Box<ArithmeticExpr>,
    ),
    /// An assignment operation.
    Assignment(ArithmeticTarget, Box<ArithmeticExpr>),
    /// A binary assignment operation.
    BinaryAssignment(BinaryOperator, ArithmeticTarget, Box<ArithmeticExpr>),
    /// A unary assignment operation.
    UnaryAssignment(UnaryAssignmentOperator, ArithmeticTarget),
}

#[cfg(feature = "fuzz-testing")]
impl<'a> arbitrary::Arbitrary<'a> for ArithmeticExpr {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        let variant = u.choose(&[
            "Literal",
            "Reference",
            "UnaryOp",
            "BinaryOp",
            "Conditional",
            "Assignment",
            "BinaryAssignment",
            "UnaryAssignment",
        ])?;

        match *variant {
            "Literal" => Ok(ArithmeticExpr::Literal(i64::arbitrary(u)?)),
            "Reference" => Ok(ArithmeticExpr::Reference(ArithmeticTarget::arbitrary(u)?)),
            "UnaryOp" => Ok(ArithmeticExpr::UnaryOp(
                UnaryOperator::arbitrary(u)?,
                Box::new(ArithmeticExpr::arbitrary(u)?),
            )),
            "BinaryOp" => Ok(ArithmeticExpr::BinaryOp(
                BinaryOperator::arbitrary(u)?,
                Box::new(ArithmeticExpr::arbitrary(u)?),
                Box::new(ArithmeticExpr::arbitrary(u)?),
            )),
            "Conditional" => Ok(ArithmeticExpr::Conditional(
                Box::new(ArithmeticExpr::arbitrary(u)?),
                Box::new(ArithmeticExpr::arbitrary(u)?),
                Box::new(ArithmeticExpr::arbitrary(u)?),
            )),
            "Assignment" => Ok(ArithmeticExpr::Assignment(
                ArithmeticTarget::arbitrary(u)?,
                Box::new(ArithmeticExpr::arbitrary(u)?),
            )),
            "BinaryAssignment" => Ok(ArithmeticExpr::BinaryAssignment(
                BinaryOperator::arbitrary(u)?,
                ArithmeticTarget::arbitrary(u)?,
                Box::new(ArithmeticExpr::arbitrary(u)?),
            )),
            "UnaryAssignment" => Ok(ArithmeticExpr::UnaryAssignment(
                UnaryAssignmentOperator::arbitrary(u)?,
                ArithmeticTarget::arbitrary(u)?,
            )),
            _ => unreachable!(),
        }
    }
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

/// A binary arithmetic operator.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum BinaryOperator {
    /// Exponentiation (e.g., `x ** y`).
    Power,
    /// Multiplication (e.g., `x * y`).
    Multiply,
    /// Division (e.g., `x / y`).
    Divide,
    /// Modulo (e.g., `x % y`).
    Modulo,
    /// Comma (e.g., `x, y`).
    Comma,
    /// Addition (e.g., `x + y`).
    Add,
    /// Subtraction (e.g., `x - y`).
    Subtract,
    /// Bitwise left shift (e.g., `x << y`).
    ShiftLeft,
    /// Bitwise right shift (e.g., `x >> y`).
    ShiftRight,
    /// Less than (e.g., `x < y`).
    LessThan,
    /// Less than or equal to (e.g., `x <= y`).
    LessThanOrEqualTo,
    /// Greater than (e.g., `x > y`).
    GreaterThan,
    /// Greater than or equal to (e.g., `x >= y`).
    GreaterThanOrEqualTo,
    /// Equals (e.g., `x == y`).
    Equals,
    /// Not equals (e.g., `x != y`).
    NotEquals,
    /// Bitwise AND (e.g., `x & y`).
    BitwiseAnd,
    /// Bitwise exclusive OR (xor) (e.g., `x ^ y`).
    BitwiseXor,
    /// Bitwise OR (e.g., `x | y`).
    BitwiseOr,
    /// Logical AND (e.g., `x && y`).
    LogicalAnd,
    /// Logical OR (e.g., `x || y`).
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

/// A unary arithmetic operator.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum UnaryOperator {
    /// Unary plus (e.g., `+x`).
    UnaryPlus,
    /// Unary minus (e.g., `-x`).
    UnaryMinus,
    /// Bitwise not (e.g., `~x`).
    BitwiseNot,
    /// Logical not (e.g., `!x`).
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

/// A unary arithmetic assignment operator.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum UnaryAssignmentOperator {
    /// Prefix increment (e.g., `++x`).
    PrefixIncrement,
    /// Prefix increment (e.g., `--x`).
    PrefixDecrement,
    /// Postfix increment (e.g., `x++`).
    PostfixIncrement,
    /// Postfix decrement (e.g., `x--`).
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

/// Identifies the target of an arithmetic assignment expression.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "fuzz-testing", derive(arbitrary::Arbitrary))]
#[cfg_attr(test, derive(PartialEq, Eq, serde::Serialize))]
pub enum ArithmeticTarget {
    /// A named variable.
    Variable(String),
    /// An element in an array.
    ArrayElement(String, Box<ArithmeticExpr>),
}

impl Display for ArithmeticTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArithmeticTarget::Variable(name) => write!(f, "{name}"),
            ArithmeticTarget::ArrayElement(name, index) => write!(f, "{name}[{index}]"),
        }
    }
}
