use std::path::PathBuf;

/// Monolithic error type for the shell
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// A local variable was set outside of a function
    #[error("can't set local variable outside of function")]
    SetLocalVarOutsideFunction,

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
    #[error("failed to source file: {0}; {1}")]
    FailedSourcingFile(PathBuf, Box<Error>),

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

    /// The requested functionality has not yet been implemented in this shell.
    #[error("UNIMPLEMENTED: {0}")]
    Unimplemented(&'static str),

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
    RedirectionFailure(String, std::io::Error),

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
    #[error("bad substitution")]
    BadSubstitution,

    /// Invalid arguments were provided to the command.
    #[error("invalid arguments")]
    InvalidArguments,

    /// An error occurred while creating a child process.
    #[error("failed to create child process")]
    ChildCreationFailure,

    /// An error occurred while formatting a string.
    #[error("{0}")]
    FormattingError(#[from] std::fmt::Error),

    /// An error occurred while parsing a word.
    #[error("{0}")]
    WordParseError(#[from] brush_parser::WordParseError),

    /// Unable to parse a test command.
    #[error("{0}")]
    TestCommandParseError(#[from] brush_parser::TestCommandParseError),

    /// A threading error occurred.
    #[error("threading error")]
    ThreadingError(#[from] tokio::task::JoinError),

    /// An invalid signal was referenced.
    #[error("{0}: invalid signal specification")]
    InvalidSignal(String),

    /// A system error occurred.
    #[cfg(unix)]
    #[error("system error: {0}")]
    ErrnoError(#[from] nix::errno::Errno),

    /// An invalid umask was provided.
    #[error("invalid umask value")]
    InvalidUmask,

    /// An error occurred reading from procfs.
    #[cfg(target_os = "linux")]
    #[error("procfs error: {0}")]
    ProcfsError(#[from] procfs::ProcError),

    /// The given open file cannot be read from.
    #[error("cannot read from {0}")]
    OpenFileNotReadable(&'static str),

    /// The given open file cannot be written to.
    #[error("cannot write to {0}")]
    OpenFileNotWritable(&'static str),

    /// Bad file descriptor.
    #[error("bad file descriptor: {0}")]
    BadFileDescriptor(u32),

    /// Printf failure
    #[error("printf failure: {0}")]
    PrintfFailure(i32),

    /// Interrupted
    #[error("interrupted")]
    Interrupted,

    /// Maximum function call depth was exceeded.
    #[error("maximum function call depth exceeded")]
    MaxFunctionCallDepthExceeded,
}

/// Convenience function for returning an error for unimplemented functionality.
///
/// # Arguments
///
/// * `msg` - The message to include in the error
pub(crate) fn unimp<T>(msg: &'static str) -> Result<T, Error> {
    Err(Error::Unimplemented(msg))
}
