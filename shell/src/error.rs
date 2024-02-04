use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("can't set local variable outside of function")]
    SetLocalVarOutsideFunction,

    #[error("cannot expand tilde expression with HOME not set")]
    TildeWithoutValidHome,

    #[error("failed to source file: {0}")]
    FailedSourcingFile(PathBuf, std::io::Error),

    #[error("UNIMPLEMENTED: {0}")]
    Unimplemented(&'static str),

    #[error("arithmetic evaluation error")]
    EvalError(#[from] crate::arithmetic::EvalError),

    #[error("failed to parse integer")]
    IntParseError(#[from] std::num::ParseIntError),

    #[error("unknown error")]
    Unknown(#[from] anyhow::Error),

    #[error("{0}")]
    IoError(#[from] std::io::Error),
}

pub(crate) fn unimp<T>(msg: &'static str) -> Result<T, Error> {
    Err(Error::Unimplemented(msg))
}
