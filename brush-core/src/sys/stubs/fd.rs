//! File descriptor utilities.

use crate::{error, openfiles};

/// Stub implementation for platforms that do not support enumerating file descriptors.
pub fn try_iter_open_fds() -> impl Iterator<Item = (i32, openfiles::OpenFile)> {
    std::iter::empty()
}

/// Stub implementation for platforms that do not support opening file descriptors.
pub fn try_get_file_for_open_fd(_fd: i32) -> Option<openfiles::OpenFile> {
    None
}
