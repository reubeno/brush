use std::path::PathBuf;

/// Represents an error encountered while running or otherwise managing an interactive shell.
#[derive(thiserror::Error, Debug)]
pub enum ShellError {
    /// An error occurred with the embedded shell.
    #[error("{0}")]
    ShellError(#[from] brush_core::Error),

    /// A generic I/O error occurred.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Failed to create xtrace file.
    #[error("failed to create xtrace file '{0}': {1}")]
    FailedToCreateXtraceFile(PathBuf, std::io::Error),

    /// An error occurred while reading input.
    #[error("input error occurred: {0}")]
    InputError(std::io::Error),

    /// The requested input backend type is not supported.
    #[error("requested input backend type not supported")]
    InputBackendNotSupported,
}
