#[cfg(unix)]
pub(crate) mod unix;
#[cfg(unix)]
pub(crate) use unix as platform;

#[cfg(windows)]
pub(crate) mod windows;
#[cfg(windows)]
pub(crate) use windows as platform;

#[cfg(target_family = "wasm")]
pub(crate) mod wasm;
#[cfg(target_family = "wasm")]
pub(crate) use wasm as platform;

#[cfg(not(unix))]
pub(crate) mod stubs;

#[cfg(any(unix, windows))]
pub(crate) mod hostname;
#[cfg(any(unix, windows))]
pub(crate) mod os_pipe;
#[cfg(any(unix, windows))]
pub(crate) mod tokio_process;

pub(crate) mod fs;

pub(crate) use platform::network;
pub(crate) use platform::pipes;
pub(crate) use platform::process;
pub(crate) use platform::resource;
pub(crate) use platform::signal;
pub(crate) use platform::terminal;
pub(crate) use platform::users;
