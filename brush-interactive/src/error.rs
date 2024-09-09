/// Represents an error encountered while running or otherwise managing an interactive shell.
#[allow(clippy::module_name_repetitions)]
#[allow(clippy::enum_variant_names)]
#[derive(thiserror::Error, Debug)]
pub enum ShellError {
    /// An error occurred with the embedded shell.
    #[error("{0}")]
    ShellError(#[from] brush_core::Error),

    /// A generic I/O error occurred.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// An error occurred while reading input.
    #[error("input error occurred")]
    InputError,

    /// The requested input backend type is not supported.
    #[error("requested input backend type not supported")]
    InputBackendNotSupported,
}
