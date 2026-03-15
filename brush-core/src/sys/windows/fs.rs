//! Filesystem utilities for Windows.

pub use crate::sys::stubs::fs::*;

/// Splits a platform-specific PATH-like value into individual paths.
///
/// On Windows, this delegates to [`std::env::split_paths`].
pub fn split_paths<T: AsRef<std::ffi::OsStr> + ?Sized>(s: &T) -> std::env::SplitPaths<'_> {
    std::env::split_paths(s)
}
