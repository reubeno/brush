//! File descriptor utilities.

use crate::{error, openfiles};

/// Stub implementation for platforms that do not support enumerating file descriptors.
pub fn try_iter_fds() -> impl Iterator<Item = (u32, openfiles::OpenFile)> {
    std::iter::empty()
}
