//! File descriptor polling utilities for timeout support.

use std::os::fd::BorrowedFd;
use std::time::Duration;

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};

use crate::openfiles::OpenFile;

/// Polls a file descriptor for input readability with a timeout.
///
/// Returns `Ok(true)` if data is available, `Ok(false)` if timeout elapsed.
fn poll_fd_for_input(fd: BorrowedFd<'_>, timeout: Duration) -> std::io::Result<bool> {
    let timeout_ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
    let poll_timeout = PollTimeout::try_from(timeout_ms).unwrap_or(PollTimeout::MAX);
    let mut poll_fds = [PollFd::new(fd, PollFlags::POLLIN)];

    loop {
        match poll(&mut poll_fds, poll_timeout) {
            Ok(0) => return Ok(false), // Timeout
            Ok(_) => {
                let revents = poll_fds[0].revents().unwrap_or(PollFlags::empty());
                // POLLIN means data available. POLLHUP/POLLERR without POLLIN means
                // EOF/error - return true so caller reads and gets the proper result.
                return Ok(
                    revents.intersects(PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR)
                );
            }
            Err(nix::errno::Errno::EINTR) => continue, // Retry on signal
            Err(e) => return Err(std::io::Error::from_raw_os_error(e as i32)),
        }
    }
}

/// Checks if a file descriptor refers to a regular file.
///
/// Regular files are always "ready" for reading (poll has no effect).
fn is_regular_file(fd: BorrowedFd<'_>) -> bool {
    match nix::sys::stat::fstat(fd) {
        Ok(stat) => {
            use nix::sys::stat::SFlag;
            SFlag::from_bits_truncate(stat.st_mode).contains(SFlag::S_IFREG)
        }
        Err(_) => false,
    }
}

/// Polls an open file for input readability with a timeout.
///
/// Returns `Ok(true)` if data is available for reading, `Ok(false)` if the timeout
/// elapsed without data becoming available.
///
/// For regular files, always returns `Ok(true)` immediately since they're always
/// "ready" (matching bash behavior where `-t` has no effect on regular files).
///
/// # Arguments
///
/// * `file` - The open file to poll.
/// * `timeout` - Maximum time to wait. Use `Duration::ZERO` to check without blocking.
///
/// # Errors
///
/// Returns an error if polling fails or the file descriptor cannot be borrowed.
pub fn poll_for_input(file: &OpenFile, timeout: Duration) -> std::io::Result<bool> {
    let fd = file
        .try_borrow_as_fd()
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    // Regular files are always ready - timeout has no effect (bash behavior).
    if is_regular_file(fd) {
        return Ok(true);
    }

    poll_fd_for_input(fd, timeout)
}
