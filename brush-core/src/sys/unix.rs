pub mod async_pipe;
pub mod commands;
pub(crate) mod env;
pub mod fd;
pub mod fs;
pub mod input;
pub(crate) mod network;
pub mod poll;
use crate::error;
pub use crate::sys::tokio_process as process;
pub mod resource;
pub mod signal;
pub mod terminal;
#[cfg(not(target_os = "ios"))]
pub(crate) mod users;
#[cfg(target_os = "ios")]
#[path = "unix/users_ios.rs"]
pub(crate) mod users;

/// Platform-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    /// A system error occurred.
    #[error("system error: {0}")]
    ErrnoError(#[from] nix::errno::Errno),
}

impl From<nix::errno::Errno> for error::ErrorKind {
    fn from(err: nix::errno::Errno) -> Self {
        PlatformError::ErrnoError(err).into()
    }
}
