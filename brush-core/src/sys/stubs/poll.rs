//! Stub file descriptor polling utilities for platforms without poll support.

use std::time::Duration;

use crate::openfiles::OpenFile;

/// Stub implementation that always returns an unsupported error.
///
/// Timeout-based reading is not supported on this platform.
pub fn poll_for_input(_file: &OpenFile, _timeout: Duration) -> std::io::Result<bool> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "poll-based timeout is not supported on this platform",
    ))
}

/// Stub implementation that always returns an unsupported error.
///
/// Timeout-based reading is not supported on this platform.
#[allow(clippy::unused_async, reason = "matches the platforms that poll")]
pub async fn poll_for_input_async(file: &OpenFile, timeout: Duration) -> std::io::Result<bool> {
    poll_for_input(file, timeout)
}
