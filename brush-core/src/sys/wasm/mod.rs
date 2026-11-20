pub mod async_pipe;
pub(crate) mod env;
pub use crate::sys::stubs::fd;
pub(crate) mod fs;
pub use crate::sys::stubs::input;
pub(crate) use crate::sys::stubs::network;
pub(crate) mod pipes;
pub use crate::sys::stubs::poll;
pub mod process;
pub use crate::sys::stubs::resource;
pub use crate::sys::stubs::signal;
pub(crate) mod tasks;
pub use crate::sys::stubs::terminal;
pub(crate) use crate::sys::stubs::users;

/// Platform-specific errors.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {}
