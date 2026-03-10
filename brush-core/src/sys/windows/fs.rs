//! Filesystem utilities for Windows.

pub use crate::sys::stubs::fs::*;

use crate::error;

/// Splits a platform-specific PATH-like value into individual paths.
///
/// On Windows, this delegates to [`std::env::split_paths`].
pub fn split_paths<T: AsRef<std::ffi::OsStr> + ?Sized>(s: &T) -> std::env::SplitPaths<'_> {
    std::env::split_paths(s)
}

/// Opens a null file that will discard all I/O.
pub fn open_null_file() -> Result<std::fs::File, error::Error> {
    let f = std::fs::File::options()
        .read(true)
        .write(true)
        .open("NUL")?;
    Ok(f)
}
