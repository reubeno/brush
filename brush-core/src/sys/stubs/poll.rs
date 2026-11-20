//! Stub file descriptor polling utilities for platforms without poll support.

use std::time::Duration;

use crate::openfiles::OpenFile;

/// Stub implementation: there's no cheap way to tell how much input is waiting.
pub fn pending_bytes(_file: &OpenFile) -> Option<usize> {
    None
}

/// Stub implementation that always returns an unsupported error.
///
/// Timeout-based reading is not supported on this platform.
pub fn poll_for_input(_file: &OpenFile, _timeout: Duration) -> std::io::Result<bool> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "poll-based timeout is not supported on this platform",
    ))
}
