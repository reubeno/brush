use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("can't set local variable outside of function")]
    SetLocalVarOutsideFunction,

    #[error("failed to source file: {0}")]
    FailedSourcingFile(PathBuf, std::io::Error),

    #[error("UNIMPLEMENTED: {0}")]
    Unimplemented(&'static str),

    #[error("arithmetic evaluation error")]
    EvalError(#[from] crate::arithmetic::EvalError),

    #[error("unknown error")]
    Unknown(#[from] anyhow::Error),

    #[error("I/O error")]
    IoError(#[from] std::io::Error),
}
