//! Platform abstraction facilities

#![allow(unused)]

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
pub(crate) mod os_pipes;
#[cfg(any(unix, windows))]
pub mod tokio_process;
#[cfg(any(unix, windows))]
pub(crate) mod tokio_tasks;

pub mod fs;

pub use platform::async_pipe;
#[cfg(any(unix, windows))]
pub(crate) use platform::commands;
pub(crate) use platform::env;
pub use platform::fd;
pub use platform::input;
pub(crate) use platform::network;
pub(crate) use platform::pipes;
pub use platform::poll;
pub use platform::process;
pub use platform::resource;
pub use platform::signal;
pub(crate) use platform::tasks;
pub use platform::terminal;
pub(crate) use platform::users;

pub use platform::PlatformError;

/// Retrieves the host process's environment variables as strings.
///
/// A name or value that isn't valid Unicode has what's invalid replaced with U+FFFD, and
/// a warning is logged; [`std::env::vars()`] would panic on it.
pub(crate) fn lossy_host_env_vars() -> impl Iterator<Item = (String, String)> {
    std::env::vars_os().map(|(name, value)| {
        let name = name.to_string_lossy();
        let value = match value.into_string() {
            Ok(value) => value,
            Err(value) => {
                tracing::warn!("environment variable {name} is not valid Unicode; altering it");
                value.to_string_lossy().into_owned()
            }
        };
        if name.contains(char::REPLACEMENT_CHARACTER) {
            tracing::warn!("environment variable name {name} is not valid Unicode; altering it");
        }
        (name.into_owned(), value)
    })
}
