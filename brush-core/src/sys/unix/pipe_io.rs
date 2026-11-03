//! Reads and writes on pipes that don't hold up the runtime.
//!
//! The other end of a pipe is often one of the shell's own tasks: the reader of a
//! command substitution's output, the writer of a process substitution's. A read or
//! write that blocks the thread such a task needs never finishes, and neither does
//! the task. So a read or write is first tried in a way that can't block, and only if
//! it would block is it done in a way that lets the runtime run its other tasks
//! elsewhere meanwhile.
//!
//! The descriptors themselves stay blocking: they're shared with child processes,
//! which expect them to be.
//!
//! The tries never sleep, so they can't be interrupted by a signal; nothing here
//! retries on `EINTR`.

use std::io::{self, Read as _, Write as _};
use std::os::fd::{AsFd as _, BorrowedFd};

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};

/// Reads from `reader` into `buf`, as [`std::io::Read::read`] does.
pub(crate) fn read(reader: &std::io::PipeReader, buf: &mut [u8]) -> io::Result<usize> {
    match try_read(reader.as_fd(), buf)? {
        Some(count) => Ok(count),
        None => without_holding_up_the_runtime(|| { reader }.read(buf)),
    }
}

/// Writes `buf` to `writer`, as [`std::io::Write::write`] does.
pub(crate) fn write(writer: &std::io::PipeWriter, buf: &[u8]) -> io::Result<usize> {
    match try_write(writer.as_fd(), buf)? {
        Some(count) => Ok(count),
        None => without_holding_up_the_runtime(|| { writer }.write(buf)),
    }
}

/// Opens `path` as a redirection target. Opening a FIFO waits for its other end, which
/// may be one of the shell's own tasks, so that open doesn't hold up the runtime. A
/// FIFO opened for only reading or only writing comes back as a pipe's end, so that
/// reads and writes on it don't either.
pub(crate) fn open_file(
    options: &std::fs::OpenOptions,
    path: &std::path::Path,
) -> io::Result<crate::openfiles::OpenFile> {
    if !is_fifo(path) {
        return Ok(options.open(path)?.into());
    }

    let file = without_holding_up_the_runtime(|| options.open(path))?;
    let mode = rustix::fs::fcntl_getfl(&file)? & rustix::fs::OFlags::RWMODE;
    Ok(if mode == rustix::fs::OFlags::RDONLY {
        std::io::PipeReader::from(std::os::fd::OwnedFd::from(file)).into()
    } else if mode == rustix::fs::OFlags::WRONLY {
        std::io::PipeWriter::from(std::os::fd::OwnedFd::from(file)).into()
    } else {
        file.into()
    })
}

/// Returns whether `path` names a FIFO.
pub(crate) fn is_fifo(path: &std::path::Path) -> bool {
    use std::os::unix::fs::FileTypeExt as _;

    std::fs::metadata(path).is_ok_and(|m| m.file_type().is_fifo())
}

/// Runs `f`, which may block, so that the runtime's other tasks can still run: on a
/// runtime with several workers, this one hands its tasks to another thread first. On
/// a runtime with a single thread there's nowhere to hand them to, so `f` just runs.
pub(crate) fn without_holding_up_the_runtime<R>(f: impl FnOnce() -> R) -> R {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(f)
        }
        _ => f(),
    }
}

/// Reads what's there to read without blocking, or returns `None` if the read would block.
fn try_read(fd: BorrowedFd<'_>, buf: &mut [u8]) -> io::Result<Option<usize>> {
    #[cfg(target_os = "linux")]
    if let Some(result) = nowait::attempt(|| {
        let bufs = &mut [io::IoSliceMut::new(&mut *buf)];
        rustix::io::preadv2(fd, bufs, nowait::CURRENT_OFFSET, nowait::FLAGS)
    }) {
        return result;
    }

    try_read_ready(fd, buf)
}

/// Writes what fits without blocking, or returns `None` if the write would block.
fn try_write(fd: BorrowedFd<'_>, buf: &[u8]) -> io::Result<Option<usize>> {
    #[cfg(target_os = "linux")]
    if let Some(result) = nowait::attempt(|| {
        let bufs = &[io::IoSlice::new(buf)];
        rustix::io::pwritev2(fd, bufs, nowait::CURRENT_OFFSET, nowait::FLAGS)
    }) {
        return result;
    }

    try_write_ready(fd, buf)
}

/// [`try_read`] for when a read can't be told not to block: reads only what `FIONREAD`
/// says is there, or the end of the input once the writers are gone.
fn try_read_ready(fd: BorrowedFd<'_>, buf: &mut [u8]) -> io::Result<Option<usize>> {
    let ready =
        rustix::io::ioctl_fionread(fd).map_or(0, |n| usize::try_from(n).unwrap_or(usize::MAX));
    let count = if ready > 0 {
        ready.min(buf.len())
    } else if polls(fd, PollFlags::POLLIN)? {
        buf.len()
    } else {
        return Ok(None);
    };
    // Another reader (a child sharing the pipe) may have taken what was there since the
    // check, in which case this read blocks: let the runtime's other tasks go on meanwhile.
    Ok(Some(without_holding_up_the_runtime(|| {
        rustix::io::read(fd, &mut buf[..count])
    })?))
}

/// [`try_write`] for when a write can't be told not to block: a pipe that polls
/// writable takes `PIPE_BUF` bytes without blocking, short of another writer filling
/// it first; one whose reader is gone polls as such, and the write fails right away.
fn try_write_ready(fd: BorrowedFd<'_>, buf: &[u8]) -> io::Result<Option<usize>> {
    if polls(fd, PollFlags::POLLOUT)? {
        let count = buf.len().min(libc::PIPE_BUF);
        // Another writer (a child sharing the pipe) may have filled it since the check, in
        // which case this write blocks: let the runtime's other tasks go on meanwhile.
        Ok(Some(without_holding_up_the_runtime(|| {
            rustix::io::write(fd, &buf[..count])
        })?))
    } else {
        Ok(None)
    }
}

/// Returns whether `fd` is ready for `events`, or has failed or been hung up on.
fn polls(fd: BorrowedFd<'_>, events: PollFlags) -> io::Result<bool> {
    let mut fds = [PollFd::new(fd, events)];
    poll(&mut fds, PollTimeout::ZERO)?;
    let revents = fds[0].revents().unwrap_or(PollFlags::empty());
    Ok(revents.intersects(events | PollFlags::POLLERR | PollFlags::POLLHUP | PollFlags::POLLNVAL))
}

/// Reads and writes told not to block (`RWF_NOWAIT`), on a descriptor that otherwise does.
#[cfg(target_os = "linux")]
mod nowait {
    use std::sync::atomic::{AtomicBool, Ordering};

    use rustix::io::{Errno, ReadWriteFlags};

    /// Has `preadv2` and `pwritev2` use the descriptor's own offset, as pipes require.
    pub(super) const CURRENT_OFFSET: u64 = u64::MAX;
    pub(super) const FLAGS: ReadWriteFlags = ReadWriteFlags::NOWAIT;

    /// Cleared once the kernel turns out not to take `RWF_NOWAIT` on pipes.
    static SUPPORTED: AtomicBool = AtomicBool::new(true);

    /// Runs `op`, returning how much it read or wrote, `None` inside if it would have
    /// blocked, or `None` if the kernel can't tell it not to block.
    pub(super) fn attempt(
        op: impl FnOnce() -> rustix::io::Result<usize>,
    ) -> Option<std::io::Result<Option<usize>>> {
        if !SUPPORTED.load(Ordering::Relaxed) {
            return None;
        }
        match op() {
            Ok(count) => Some(Ok(Some(count))),
            Err(Errno::AGAIN) => Some(Ok(None)),
            // An older kernel, one without `RWF_NOWAIT` for pipes, or a seccomp filter
            // that turns the call away.
            Err(Errno::OPNOTSUPP | Errno::INVAL | Errno::NOSYS | Errno::PERM) => {
                SUPPORTED.store(false, Ordering::Relaxed);
                None
            }
            Err(e) => Some(Err(e.into())),
        }
    }
}

#[cfg(test)]
#[allow(clippy::panic_in_result_fn, reason = "assertions in a fallible test")]
mod tests {
    use std::io::Write as _;

    use super::*;

    type TryRead = fn(BorrowedFd<'_>, &mut [u8]) -> io::Result<Option<usize>>;
    type TryWrite = fn(BorrowedFd<'_>, &[u8]) -> io::Result<Option<usize>>;

    /// Each way of trying: the one this platform uses, and the one for platforms where a
    /// read or write can't be told not to block.
    const STRATEGIES: [(&str, TryRead, TryWrite); 2] = [
        ("default", try_read, try_write),
        ("ready", try_read_ready, try_write_ready),
    ];

    /// Fills `writer`'s pipe until a write would block, returning how much it took.
    fn fill(writer: &std::io::PipeWriter, try_write: TryWrite) -> io::Result<usize> {
        let chunk = vec![b'x'; 65536];
        let mut total = 0;
        while let Some(count) = try_write(writer.as_fd(), &chunk)? {
            total += count;
        }
        Ok(total)
    }

    #[test]
    fn a_read_from_an_empty_pipe_would_block() -> io::Result<()> {
        for (name, try_read, _) in STRATEGIES {
            let (reader, _writer) = std::io::pipe()?;
            assert_eq!(try_read(reader.as_fd(), &mut [0; 16])?, None, "{name}");
        }
        Ok(())
    }

    #[test]
    fn a_read_takes_what_is_there() -> io::Result<()> {
        for (name, try_read, _) in STRATEGIES {
            let (reader, mut writer) = std::io::pipe()?;
            writer.write_all(b"hello")?;
            let mut buf = [0; 16];
            assert_eq!(try_read(reader.as_fd(), &mut buf)?, Some(5), "{name}");
            assert_eq!(&buf[..5], b"hello", "{name}");
            assert_eq!(try_read(reader.as_fd(), &mut buf)?, None, "{name}");
        }
        Ok(())
    }

    #[test]
    fn a_read_after_the_writers_are_gone_is_the_end() -> io::Result<()> {
        for (name, try_read, _) in STRATEGIES {
            let (reader, writer) = std::io::pipe()?;
            drop(writer);
            assert_eq!(try_read(reader.as_fd(), &mut [0; 16])?, Some(0), "{name}");
        }
        Ok(())
    }

    #[test]
    fn a_write_to_a_full_pipe_would_block() -> io::Result<()> {
        for (name, _, try_write) in STRATEGIES {
            let (reader, writer) = std::io::pipe()?;
            let total = fill(&writer, try_write)?;
            assert!(total > 0, "{name}");
            assert_eq!(try_write(writer.as_fd(), b"x")?, None, "{name}");

            // Once enough is read, there's room again.
            let mut buf = vec![0; 1 << 20];
            while try_write(writer.as_fd(), b"x")?.is_none() {
                assert!(read(&reader, &mut buf)? > 0, "{name}");
            }
        }
        Ok(())
    }

    #[test]
    fn a_write_without_a_reader_fails() -> io::Result<()> {
        let (reader, writer) = std::io::pipe()?;
        drop(reader);
        // Rust starts processes with SIGPIPE ignored, so the write reports the broken pipe.
        let err = write(&writer, b"x").err().map(|e| e.kind());
        assert_eq!(err, Some(io::ErrorKind::BrokenPipe));
        Ok(())
    }

    /// The descriptors stay blocking, as the processes that inherit them expect.
    #[test]
    fn the_descriptors_stay_blocking() -> io::Result<()> {
        for (name, try_read, try_write) in STRATEGIES {
            let (reader, writer) = std::io::pipe()?;
            let _ = fill(&writer, try_write)?;
            let _ = try_read(reader.as_fd(), &mut [0; 16])?;
            for fd in [reader.as_fd(), writer.as_fd()] {
                let flags = nix::fcntl::fcntl(fd, nix::fcntl::FcntlArg::F_GETFL)?;
                assert_eq!(flags & libc::O_NONBLOCK, 0, "{name}");
            }
        }
        Ok(())
    }

    /// A FIFO opened for only reading or only writing comes back as a pipe's end;
    /// one opened for both, and anything that isn't a FIFO, as a file.
    #[test]
    fn a_fifo_opens_as_a_pipe_end() -> io::Result<()> {
        use crate::openfiles::OpenFile;

        let dir = tempfile::tempdir()?;
        let fifo = dir.path().join("p");
        nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU)?;
        let regular = dir.path().join("f");
        std::fs::write(&regular, b"")?;

        let mut both = std::fs::OpenOptions::new();
        both.read(true).write(true);
        // Opening for both doesn't wait for another end, so open that one first.
        let rw = open_file(&both, &fifo)?;
        let r = open_file(std::fs::OpenOptions::new().read(true), &fifo)?;
        let w = open_file(std::fs::OpenOptions::new().write(true), &fifo)?;
        let file = open_file(std::fs::OpenOptions::new().read(true), &regular)?;

        assert!(matches!(rw, OpenFile::File(_)));
        assert!(matches!(r, OpenFile::PipeReader(_)));
        assert!(matches!(w, OpenFile::PipeWriter(_)));
        assert!(matches!(file, OpenFile::File(_)));
        Ok(())
    }

    /// A full pipe's writer lets the runtime's other tasks run, even the reader that
    /// would make room, on a runtime with a single worker.
    #[test]
    fn a_blocked_write_lets_the_reader_run() -> io::Result<()> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()?;
        runtime.block_on(async {
            use std::io::Read as _;

            let (reader, writer) = std::io::pipe()?;
            let mut reader = crate::openfiles::OpenFile::from(reader);
            let mut writer = crate::openfiles::OpenFile::from(writer);
            let data = vec![b'x'; 4 << 20];
            let expected = data.len();
            let writer_task = tokio::spawn(async move { writer.write_all(&data) });
            let reader_task = tokio::spawn(async move {
                let mut all = Vec::new();
                reader.read_to_end(&mut all).map(|_| all.len())
            });
            writer_task.await.map_err(io::Error::other)??;
            assert_eq!(reader_task.await.map_err(io::Error::other)??, expected);
            io::Result::Ok(())
        })
    }
}
