//! File descriptor polling utilities for timeout support.

use std::os::fd::{AsFd as _, BorrowedFd};
use std::time::{Duration, Instant};

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};

use crate::openfiles::OpenFile;

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

    poll_fd_for_input(fd, None, deadline)
}

/// [`poll_for_input`] for an async caller: waits for input without blocking the thread
/// it's awaited on, on any runtime, and without needing the runtime's timers.
///
/// If there's input right away, or no time to wait for it, this answers right away.
/// Otherwise it waits on a thread of its own, polling a duplicate of the descriptor. If
/// the returned future is dropped before then, that thread stops waiting right away
/// rather than at the deadline.
///
/// # Arguments
///
/// * `file` - The open file to poll.
/// * `timeout` - Maximum time to wait. Use `Duration::ZERO` to check without blocking.
///
/// # Errors
///
/// Returns an error if polling fails or the file descriptor cannot be borrowed.
pub async fn poll_for_input_async(file: &OpenFile, timeout: Duration) -> std::io::Result<bool> {
    let ready = poll_for_input(file, Duration::ZERO)?;
    if ready || timeout.is_zero() {
        return Ok(ready);
    }

    let fd = file
        .try_borrow_as_fd()
        .map_err(|e| std::io::Error::other(e.to_string()))?
        .try_clone_to_owned()?;
    let deadline = Instant::now().checked_add(timeout);

    // The waiting thread also watches this pipe's read end. Its write end is held here,
    // so if this future is dropped, the write end is closed, and the wait ends.
    let (cancel_reader, cancel_writer) = std::io::pipe()?;
    let ready = tokio::task::spawn_blocking(move || {
        poll_fd_for_input(fd.as_fd(), Some(cancel_reader.as_fd()), deadline)
    })
    .await
    .map_err(std::io::Error::other)?;
    drop(cancel_writer);
    ready
}

/// Polls a file descriptor for input readability with a deadline.
///
/// Returns `Ok(true)` if data is available, `Ok(false)` if deadline passed.
///
/// # Arguments
///
/// * `fd` - File descriptor to poll
/// * `cancel` - Optional descriptor that ends the wait, as if it timed out, once it's
///   readable or hung up on.
/// * `deadline` - Optional deadline; `None` indicates no deadline.
fn poll_fd_for_input(
    fd: BorrowedFd<'_>,
    cancel: Option<BorrowedFd<'_>>,
    deadline: Option<Instant>,
) -> std::io::Result<bool> {
    let mut poll_fds: Vec<PollFd<'_>> = std::iter::once(fd)
        .chain(cancel)
        .map(|fd| PollFd::new(fd, PollFlags::POLLIN))
        .collect();
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

        // Check first without waiting; only a wait for input, which may come from one of
        // the shell's own tasks, needs to let the runtime run its other tasks meanwhile.
        let polled = match poll(&mut poll_fds, PollTimeout::ZERO) {
            Ok(0) if timeout_ms != 0 => crate::sys::pipe_io::without_holding_up_the_runtime(|| {
                poll(&mut poll_fds, poll_timeout)
            }),
            other => other,
        };
        match polled {
            Ok(0) => return Ok(false), // Timeout
            Ok(_) => {
                if poll_fds
                    .get(1)
                    .and_then(PollFd::revents)
                    .is_some_and(|r| !r.is_empty())
                {
                    return Ok(false); // Cancelled.
                }
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

/// Checks if a file descriptor refers to a regular file.
///
/// Regular files are always "ready" for reading (poll has no effect).
///
/// # Arguments
///
/// * `fd` - File descriptor to check
fn is_regular_file(fd: BorrowedFd<'_>) -> bool {
    match nix::sys::stat::fstat(fd) {
        Ok(stat) => {
            use nix::sys::stat::{SFlag, mode_t};
            // By the whole file type: a socket's, for one, has the regular file's bit set.
            mode_t::try_from(stat.st_mode).is_ok_and(|mode| {
                (SFlag::from_bits_truncate(mode) & SFlag::S_IFMT) == SFlag::S_IFREG
            })
        }
        Err(_) => false,
    }
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn, reason = "assertions in a fallible test")]
mod tests {
    use std::os::fd::AsFd as _;

    use super::*;

    /// Only a regular file is one: a socket's file type has the regular file's bit set among
    /// others, and a read from one must poll, not take the input to be ready.
    #[test]
    fn only_a_regular_file_is_regular() -> std::io::Result<()> {
        let file = tempfile::tempfile()?;
        assert!(is_regular_file(file.as_fd()));

        let (socket, _other_end) = std::os::unix::net::UnixStream::pair()?;
        assert!(!is_regular_file(socket.as_fd()));

        let (reader, _writer) = std::io::pipe()?;
        assert!(!is_regular_file(reader.as_fd()));
        Ok(())
    }

    /// Polling a socket with nothing to read waits out the timeout, as `read -t` from one does.
    #[test]
    fn a_socket_without_input_times_out() -> std::io::Result<()> {
        let (socket, _other_end) = std::os::unix::net::UnixStream::pair()?;
        let socket: OpenFile = std::fs::File::from(std::os::fd::OwnedFd::from(socket)).into();

        assert!(!poll_for_input(&socket, Duration::from_millis(50))?);
        Ok(())
    }

    /// A wait that's given up on stops right away, not at its deadline: the runtime,
    /// which waits for its blocking threads when dropped, drops promptly.
    #[test]
    fn a_dropped_wait_stops_waiting() -> std::io::Result<()> {
        let (reader, _writer) = std::io::pipe()?;
        let reader = OpenFile::from(reader);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let waited = runtime.block_on(async {
            tokio::time::timeout(
                Duration::from_millis(50),
                poll_for_input_async(&reader, Duration::from_secs(30)),
            )
            .await
        });
        assert!(
            waited.is_err(),
            "nothing was written, so the wait times out"
        );

        let start = Instant::now();
        drop(runtime);
        assert!(start.elapsed() < Duration::from_secs(5));
        Ok(())
    }
}
