mod basic_shell;

#[cfg(any(unix, windows))]
mod term_utils;

#[cfg(target_family = "wasm")]
mod term_stubs;

#[allow(clippy::module_name_repetitions)]
pub use basic_shell::BasicShell;
