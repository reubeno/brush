use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("can't set local variable outside of function")]
    SetLocalVarOutsideFunction,

    #[error("cannot expand tilde expression with HOME not set")]
    TildeWithoutValidHome,

    #[error("cannot assign list to array member")]
    AssigningListToArrayMember,

    #[error("cannot convert associative array to indexed array")]
    ConvertingAssociativeArrayToIndexedArray,

    #[error("cannot convert indexed array to associative array")]
    ConvertingIndexedArrayToAssociativeArray,

    #[error("failed to source file: {0}")]
    FailedSourcingFile(PathBuf, std::io::Error),

    #[error("cannot assign in this way")]
    CannotAssignToSpecialParameter,

    #[error("expansion error: {0}")]
    CheckedExpansionError(String),

    #[error("UNIMPLEMENTED: {0}")]
    Unimplemented(&'static str),

    #[error("not a directory: {0}")]
    NotADirectory(PathBuf),

    #[error("invalid redirection")]
    InvalidRedirection,

    #[error("arithmetic evaluation error: {0}")]
    EvalError(#[from] crate::arithmetic::EvalError),

    #[error("failed to parse integer")]
    IntParseError(#[from] std::num::ParseIntError),

    #[error("cannot mutate readonly variable")]
    ReadonlyVariable,

    #[error("invalid pattern: '{0}'")]
    InvalidPattern(String),

    #[error("{0}")]
    Unknown(#[from] anyhow::Error),

    #[error("{0}")]
    IoError(#[from] std::io::Error),
}

pub(crate) fn unimp<T>(msg: &'static str) -> Result<T, Error> {
    Err(Error::Unimplemented(msg))
}
