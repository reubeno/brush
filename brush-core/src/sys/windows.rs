pub use crate::sys::stubs::commands;
pub use crate::sys::stubs::fd;
pub use crate::sys::stubs::fs;
pub use crate::sys::stubs::input;
pub(crate) mod network;
pub use crate::sys::stubs::resource;

/// Signal processing utilities
pub mod signal {
    pub(crate) use crate::sys::stubs::signal::*;
    pub(crate) use tokio::signal::ctrl_c as await_ctrl_c;
}

pub use crate::sys::stubs::terminal;
pub use crate::sys::tokio_process as process;
pub(crate) mod users;

/// Platform-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {}
