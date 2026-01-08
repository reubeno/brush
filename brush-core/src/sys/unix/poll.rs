//! File descriptor polling utilities for timeout support.

use std::os::fd::BorrowedFd;
use std::time::{Duration, Instant};

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};

use crate::openfiles::OpenFile;

/// Polls a file descriptor for input readability with a deadline.
///
/// Returns `Ok(true)` if data is available, `Ok(false)` if deadline passed.
fn poll_fd_for_input(fd: BorrowedFd<'_>, deadline: Option<Instant>) -> std::io::Result<bool> {
    let mut poll_fds = [PollFd::new(fd, PollFlags::POLLIN)];
    let mut first_iteration = true;

    loop {
        // Calculate remaining time on each iteration to handle EINTR correctly.
        let timeout_ms = match deadline {
            Some(d) => {
                let remaining = d.saturating_duration_since(Instant::now());
                // On first iteration, always do at least one poll even with zero timeout.
                // This allows `-t 0` to check if input is immediately available.
                if remaining.is_zero() && !first_iteration {
                    return Ok(false); // Deadline passed after initial poll.
                }
                i32::try_from(remaining.as_millis()).unwrap_or(i32::MAX)
            }
            None => -1, // Block indefinitely.
        };
        first_iteration = false;
        let poll_timeout = PollTimeout::try_from(timeout_ms).unwrap_or(PollTimeout::MAX);

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
            Err(nix::errno::Errno::EINTR) => (), // Retry on signal with recalculated timeout.
            Err(e) => return Err(std::io::Error::from_raw_os_error(e as i32)),
        }
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

    // Convert timeout to deadline for accurate time tracking across EINTR retries.
    let deadline = if timeout.is_zero() {
        // For zero timeout, use current instant so first check sees zero remaining.
        Some(Instant::now())
    } else {
        Some(Instant::now() + timeout)
    };

    poll_fd_for_input(fd, deadline)
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
