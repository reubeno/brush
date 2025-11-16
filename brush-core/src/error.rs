//! Error facilities

use std::path::PathBuf;

use crate::{Shell, ShellFd, results, sys};

/// Unified error type for this crate. Contains just a kind for now,
/// but will be extended later with additional context.
#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub struct Error {
    /// The kind of error.
    kind: ErrorKind,
}

/// Monolithic error type for the shell
#[derive(thiserror::Error, Debug)]
pub enum ErrorKind {
    /// A tilde expression was used without a valid HOME variable
    #[error("cannot expand tilde expression with HOME not set")]
    TildeWithoutValidHome,

    /// An attempt was made to assign a list to an array member
    #[error("cannot assign list to array member")]
    AssigningListToArrayMember,

    /// An attempt was made to convert an associative array to an indexed array.
    #[error("cannot convert associative array to indexed array")]
    ConvertingAssociativeArrayToIndexedArray,

    /// An attempt was made to convert an indexed array to an associative array.
    #[error("cannot convert indexed array to associative array")]
    ConvertingIndexedArrayToAssociativeArray,

    /// An error occurred while sourcing the indicated script file.
    #[error("failed to source file: {0}")]
    FailedSourcingFile(PathBuf, #[source] std::io::Error),

    /// The shell failed to send a signal to a process.
    #[error("failed to send signal to process")]
    FailedToSendSignal,

    /// An attempt was made to assign a value to a special parameter.
    #[error("cannot assign in this way")]
    CannotAssignToSpecialParameter,

    /// Checked expansion error.
    #[error("expansion error: {0}")]
    CheckedExpansionError(String),

    /// A reference was made to an unknown shell function.
    #[error("function not found: {0}")]
    FunctionNotFound(String),

    /// Command was not found.
    #[error("command not found: {0}")]
    CommandNotFound(String),

    /// Not a builtin.
    #[error("not a shell builtin: {0}")]
    BuiltinNotFound(String),

    /// The working directory does not exist.
    #[error("working directory does not exist: {0}")]
    WorkingDirMissing(PathBuf),

    /// Failed to execute command.
    #[error("failed to execute command '{0}': {1}")]
    FailedToExecuteCommand(String, #[source] std::io::Error),

    /// History item was not found.
    #[error("history item not found")]
    HistoryItemNotFound,

    /// The requested functionality has not yet been implemented in this shell.
    #[error("not yet implemented: {0}")]
    Unimplemented(&'static str),

    /// The requested functionality has not yet been implemented in this shell; it is tracked in a
    /// GitHub issue.
    #[error("not yet implemented: {0}; see https://github.com/reubeno/brush/issues/{1}")]
    UnimplementedAndTracked(&'static str, u32),

    /// An expected environment scope could not be found.
    #[error("missing scope")]
    MissingScope,

    /// The given path is not a directory.
    #[error("not a directory: {0}")]
    NotADirectory(PathBuf),

    /// The given path is a directory.
    #[error("path is a directory")]
    IsADirectory,

    /// The given variable is not an array.
    #[error("variable is not an array")]
    NotArray,

    /// The current user could not be determined.
    #[error("no current user")]
    NoCurrentUser,

    /// The requested input or output redirection is invalid.
    #[error("invalid redirection")]
    InvalidRedirection,

    /// An error occurred while redirecting input or output with the given file.
    #[error("failed to redirect to {0}: {1}")]
    RedirectionFailure(String, String),

    /// An error occurred evaluating an arithmetic expression.
    #[error("arithmetic evaluation error: {0}")]
    EvalError(#[from] crate::arithmetic::EvalError),

    /// The given string could not be parsed as an integer.
    #[error("failed to parse integer")]
    IntParseError(#[from] std::num::ParseIntError),

    /// The given string could not be parsed as an integer.
    #[error("failed to parse integer")]
    TryIntParseError(#[from] std::num::TryFromIntError),

    /// A byte sequence could not be decoded as a valid UTF-8 string.
    #[error("failed to decode utf-8")]
    FromUtf8Error(#[from] std::string::FromUtf8Error),

    /// A byte sequence could not be decoded as a valid UTF-8 string.
    #[error("failed to decode utf-8")]
    Utf8Error(#[from] std::str::Utf8Error),

    /// An attempt was made to modify a readonly variable.
    #[error("cannot mutate readonly variable")]
    ReadonlyVariable,

    /// The indicated pattern is invalid.
    #[error("invalid pattern: '{0}'")]
    InvalidPattern(String),

    /// A regular expression error occurred
    #[error("regex error: {0}")]
    RegexError(#[from] fancy_regex::Error),

    /// An invalid regular expression was provided.
    #[error("invalid regex: {0}; expression: '{1}'")]
    InvalidRegexError(fancy_regex::Error, String),

    /// An I/O error occurred.
    #[error("i/o error: {0}")]
    IoError(#[from] std::io::Error),

    /// Invalid substitution syntax.
    #[error("bad substitution: {0}")]
    BadSubstitution(String),

    /// An error occurred while creating a child process.
    #[error("failed to create child process")]
    ChildCreationFailure,

    /// An error occurred while formatting a string.
    #[error(transparent)]
    FormattingError(#[from] std::fmt::Error),

    /// An error occurred while parsing.
    #[error("{1}: {0}")]
    ParseError(brush_parser::ParseError, brush_parser::SourceInfo),

    /// An error occurred while parsing a function body.
    #[error("{0}: {1}")]
    FunctionParseError(String, brush_parser::ParseError),

    /// An error occurred while parsing a word.
    #[error(transparent)]
    WordParseError(#[from] brush_parser::WordParseError),

    /// Unable to parse a test command.
    #[error(transparent)]
    TestCommandParseError(#[from] brush_parser::TestCommandParseError),

    /// Unable to parse a key binding specification.
    #[error(transparent)]
    BindingParseError(#[from] brush_parser::BindingParseError),

    /// A threading error occurred.
    #[error("threading error")]
    ThreadingError(#[from] tokio::task::JoinError),

    /// An invalid signal was referenced.
    #[error("{0}: invalid signal specification")]
    InvalidSignal(String),

    /// A platform error occurred.
    #[error("platform error: {0}")]
    PlatformError(#[from] sys::PlatformError),

    /// An invalid umask was provided.
    #[error("invalid umask value")]
    InvalidUmask,

    /// The given open file cannot be read from.
    #[error("cannot read from {0}")]
    OpenFileNotReadable(&'static str),

    /// The given open file cannot be written to.
    #[error("cannot write to {0}")]
    OpenFileNotWritable(&'static str),

    /// Bad file descriptor.
    #[error("bad file descriptor: {0}")]
    BadFileDescriptor(ShellFd),

    /// Printf failure
    #[error("printf failure: {0}")]
    PrintfFailure(i32),

    /// Printf invalid usage
    #[error("printf: {0}")]
    PrintfInvalidUsage(String),

    /// Interrupted
    #[error("interrupted")]
    Interrupted,

    /// Maximum function call depth was exceeded.
    #[error("maximum function call depth exceeded")]
    MaxFunctionCallDepthExceeded,

    /// System time error.
    #[error("system time error: {0}")]
    TimeError(#[from] std::time::SystemTimeError),

    /// Array index out of range.
    #[error("array index out of range: {0}")]
    ArrayIndexOutOfRange(i64),

    /// Unhandled key code.
    #[error("unhandled key code: {0:?}")]
    UnhandledKeyCode(Vec<u8>),

    /// An error occurred in a built-in command.
    #[error("{1}: {0}")]
    BuiltinError(Box<dyn BuiltinError>, String),

    /// Operation not supported on this platform.
    #[error("operation not supported on this platform: {0}")]
    NotSupportedOnThisPlatform(&'static str),

    /// Command history is not enabled in this shell.
    #[error("command history is not enabled in this shell")]
    HistoryNotEnabled,

    /// Unknown key binding function.
    #[error("unknown key binding function: {0}")]
    UnknownKeyBindingFunction(String),
}

impl BuiltinError for Error {}

/// Trait implementable by built-in commands to represent errors.
pub trait BuiltinError: std::error::Error + ConvertibleToExitCode + Send + Sync {}

/// Helper trait for converting values to exit codes.
pub trait ConvertibleToExitCode {
    /// Converts to an exit code.
    fn as_exit_code(&self) -> results::ExecutionExitCode;
}

impl<T> ConvertibleToExitCode for T
where
    results::ExecutionExitCode: for<'a> From<&'a T>,
{
    fn as_exit_code(&self) -> results::ExecutionExitCode {
        self.into()
    }
}

impl From<&ErrorKind> for results::ExecutionExitCode {
    fn from(value: &ErrorKind) -> Self {
        match value {
            ErrorKind::CommandNotFound(..) => Self::NotFound,
            ErrorKind::Unimplemented(..) | ErrorKind::UnimplementedAndTracked(..) => {
                Self::Unimplemented
            }
            ErrorKind::ParseError(..) => Self::InvalidUsage,
            ErrorKind::FunctionParseError(..) => Self::InvalidUsage,
            ErrorKind::FailedToExecuteCommand(..) => Self::CannotExecute,
            ErrorKind::BuiltinError(inner, ..) => inner.as_exit_code(),
            _ => Self::GeneralError,
        }
    }
}

impl From<&Error> for results::ExecutionExitCode {
    fn from(error: &Error) -> Self {
        Self::from(&error.kind)
    }
}

impl<T> From<T> for Error
where
    ErrorKind: From<T>,
{
    fn from(convertible_to_kind: T) -> Self {
        Self {
            kind: convertible_to_kind.into(),
        }
    }
}

/// Trait implementable by consumers of this crate to customize formatting errors into
/// displayable text.
pub trait ErrorFormatter: Send {
    /// Format the given error for display within the context of the provided shell.
    ///
    /// # Arguments
    ///
    /// * `error` - The error to format.
    /// * `shell` - The shell in which the error occurred.
    fn format_error(&self, error: &Error, shell: &Shell) -> String;
}

/// Default implementation of the [`ErrorFormatter`] trait.
pub(crate) struct DefaultErrorFormatter {}

impl DefaultErrorFormatter {
    pub const fn new() -> Self {
        Self {}
    }
}

impl ErrorFormatter for DefaultErrorFormatter {
    fn format_error(&self, err: &Error, _shell: &Shell) -> String {
        std::format!("error: {err:#}\n")
    }
}

/// Convenience function for returning an error for unimplemented functionality.
///
/// # Arguments
///
/// * `msg` - The message to include in the error
pub fn unimp<T>(msg: &'static str) -> Result<T, Error> {
    Err(ErrorKind::Unimplemented(msg).into())
}

/// Convenience function for returning an error for *tracked*, unimplemented functionality.
///
/// # Arguments
///
/// * `msg` - The message to include in the error
/// * `project_issue_id` - The GitHub issue ID where the implementation is tracked.
#[allow(unused)]
pub fn unimp_with_issue<T>(msg: &'static str, project_issue_id: u32) -> Result<T, Error> {
    Err(ErrorKind::UnimplementedAndTracked(msg, project_issue_id).into())
}
