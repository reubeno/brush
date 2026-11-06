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
use std::os::fd::{AsFd as _, BorrowedFd, OwnedFd};
use std::task::{Context, Poll, ready};

use tokio::io::Interest;
use tokio::io::unix::AsyncFd;

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};

/// Reads from `reader` into `buf`, as [`std::io::Read::read`] does.
pub(crate) fn read(reader: &std::io::PipeReader, buf: &mut [u8]) -> io::Result<usize> {
    read_fd(reader.as_fd(), buf)
}

/// Writes `buf` to `writer`, as [`std::io::Write::write`] does.
pub(crate) fn write(writer: &std::io::PipeWriter, buf: &[u8]) -> io::Result<usize> {
    write_fd(writer.as_fd(), buf)
}

/// Reads from a FIFO open for reading and writing into `buf`, as [`read`] does.
pub(crate) fn read_fifo(fifo: &std::fs::File, buf: &mut [u8]) -> io::Result<usize> {
    read_fd(fifo.as_fd(), buf)
}

/// Writes `buf` to a FIFO open for reading and writing, as [`write`] does.
pub(crate) fn write_fifo(fifo: &std::fs::File, buf: &[u8]) -> io::Result<usize> {
    let fd = fifo.as_fd();
    match try_write_fifo(fd, buf)? {
        Some(count) => Ok(count),
        None => without_holding_up_the_runtime(|| Ok(rustix::io::write(fd, buf)?)),
    }
}

/// [`read`], on a pipe's (or FIFO's) descriptor.
fn read_fd(fd: BorrowedFd<'_>, buf: &mut [u8]) -> io::Result<usize> {
    match try_read(fd, buf)? {
        Some(count) => Ok(count),
        None => without_holding_up_the_runtime(|| Ok(rustix::io::read(fd, buf)?)),
    }
}

/// [`write`], on a pipe's (or FIFO's) descriptor.
fn write_fd(fd: BorrowedFd<'_>, buf: &[u8]) -> io::Result<usize> {
    match try_write(fd, buf)? {
        Some(count) => Ok(count),
        None => without_holding_up_the_runtime(|| Ok(rustix::io::write(fd, buf)?)),
    }
}

/// Opens `path` as a redirection target. Opening a FIFO waits for its other end, which
/// may be one of the shell's own tasks, so that open doesn't hold up the runtime. A FIFO
/// comes back as [`OpenFile::Fifo`](crate::openfiles::OpenFile::Fifo), whose reads and
/// writes don't either.
pub(crate) fn open_file(
    options: &std::fs::OpenOptions,
    path: &std::path::Path,
) -> io::Result<crate::openfiles::OpenFile> {
    if !is_fifo(path) {
        return Ok(options.open(path)?.into());
    }
    Ok(fifo_end(without_holding_up_the_runtime(|| {
        options.open(path)
    })?))
}

/// [`open_file`] for an async caller: a FIFO is opened on a thread of its own, which
/// the caller awaits, so the open holds up no task but the caller's, on any runtime.
pub(crate) async fn open_file_async(
    options: &std::fs::OpenOptions,
    path: &std::path::Path,
) -> io::Result<crate::openfiles::OpenFile> {
    if !is_fifo(path) {
        return Ok(options.open(path)?.into());
    }
    let (options, path) = (options.clone(), path.to_owned());
    let file = tokio::task::spawn_blocking(move || options.open(path))
        .await
        .map_err(io::Error::other)??;
    Ok(fifo_end(file))
}

/// Returns whether `path` names a FIFO.
pub(crate) fn is_fifo(path: &std::path::Path) -> bool {
    use std::os::unix::fs::FileTypeExt as _;

    std::fs::metadata(path).is_ok_and(|m| m.file_type().is_fifo())
}

/// Returns a FIFO opened by its path as [`OpenFile::Fifo`](crate::openfiles::OpenFile::Fifo).
fn fifo_end(file: std::fs::File) -> crate::openfiles::OpenFile {
    crate::openfiles::OpenFile::Fifo(std::sync::Arc::new(file))
}

/// Reads and writes on an [`OpenFile`](crate::openfiles::OpenFile) that wait, when a
/// pipe isn't ready, by giving way to the runtime's other tasks rather than blocking.
///
/// On a pipe's end, or a FIFO, a read or write that can go through right away does so,
/// as with [`read`] and [`write`]. One that would block returns [`Poll::Pending`] until
/// it can go through:
///
/// - On a pipe's end, the pipe is registered with the runtime, which says when it's
///   ready. Only then is the descriptor duplicated: the registration needs one of its
///   own.
/// - On a FIFO, the read or write is done on the runtime's blocking pool, through a
///   duplicate of the descriptor, and awaited. A FIFO's readiness can't be relied on:
///   on macOS, one whose writers are all gone reports nothing at all, to `poll` or to
///   kqueue, though a read returns the end of the input right away; and a full one may
///   poll writable. So a FIFO write is only tried without blocking where the kernel can
///   be told not to block.
///
/// Other files are read and written synchronously, as they are elsewhere. A task that
/// runs on a thread of its own (see
/// [`tasks::on_a_thread_of_its_own`](crate::tasks::on_a_thread_of_its_own)) blocks that
/// thread instead, as [`read`] and [`write`] do.
///
/// Waiting needs the runtime's I/O driver, as the rest of the shell does.
#[derive(Default)]
pub(crate) struct AsyncIo {
    registration: Registration,
    /// A FIFO read or write that's under way on the blocking pool.
    in_flight: Option<InFlight>,
}

/// Where an [`AsyncIo`] stands with registering its descriptor with the runtime.
#[derive(Default)]
enum Registration {
    /// Not tried yet: nothing has had to wait.
    #[default]
    None,
    /// Registered.
    Registered(AsyncFd<OwnedFd>),
    /// The runtime wouldn't take it, so reads and writes that would block, block.
    Refused,
}

/// A FIFO read or write under way on the blocking pool.
enum InFlight {
    /// A read, yielding what it read.
    Read(tokio::task::JoinHandle<io::Result<Vec<u8>>>),
    /// A write, yielding how much it wrote.
    Write(tokio::task::JoinHandle<io::Result<usize>>),
}

/// The most a FIFO write on the blocking pool copies of what it's given to write.
const MAX_FIFO_WRITE: usize = 64 * 1024;

impl AsyncIo {
    /// Reads from `file` into `buf`.
    pub(crate) fn poll_read(
        &mut self,
        cx: &mut Context<'_>,
        file: &mut crate::openfiles::OpenFile,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let Some((fd, wait)) = pipe_fd(file, Direction::Read) else {
            return Poll::Ready(file.read(buf));
        };
        loop {
            match self.in_flight.take() {
                Some(InFlight::Read(mut read)) => {
                    let Poll::Ready(joined) = std::pin::Pin::new(&mut read).poll(cx) else {
                        self.in_flight = Some(InFlight::Read(read));
                        return Poll::Pending;
                    };
                    let data = joined.map_err(io::Error::other)??;
                    let count = data.len().min(buf.len());
                    buf[..count].copy_from_slice(&data[..count]);
                    return Poll::Ready(Ok(count));
                }
                // A write given up on: let it finish before reading.
                Some(InFlight::Write(mut write)) => {
                    if std::pin::Pin::new(&mut write).poll(cx).is_pending() {
                        self.in_flight = Some(InFlight::Write(write));
                        return Poll::Pending;
                    }
                }
                None => (),
            }
            if let Some(count) = try_read(fd, buf)? {
                return Poll::Ready(Ok(count));
            }
            if !can_wait() {
                return Poll::Ready(read_fd(fd, buf));
            }
            match wait {
                Wait::Readiness(interest) => {
                    let Some(registration) = self.registration(fd, interest)? else {
                        return Poll::Ready(read_fd(fd, buf));
                    };
                    ready!(registration.poll_read_ready(cx))?.clear_ready();
                }
                Wait::BlockingPool => {
                    let owned = fd.try_clone_to_owned()?;
                    let len = buf.len();
                    self.in_flight = Some(InFlight::Read(tokio::task::spawn_blocking(move || {
                        let mut data = vec![0; len];
                        let count = retrying_interrupts(|| rustix::io::read(&owned, &mut data))?;
                        data.truncate(count);
                        Ok(data)
                    })));
                }
            }
        }
    }

    /// Writes `buf` to `file`.
    pub(crate) fn poll_write(
        &mut self,
        cx: &mut Context<'_>,
        file: &mut crate::openfiles::OpenFile,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let Some((fd, wait)) = pipe_fd(file, Direction::Write) else {
            return Poll::Ready(file.write(buf));
        };
        loop {
            match self.in_flight.take() {
                // Called again with the same data, as `AsyncWrite` asks after `Pending`.
                Some(InFlight::Write(mut write)) => {
                    let Poll::Ready(joined) = std::pin::Pin::new(&mut write).poll(cx) else {
                        self.in_flight = Some(InFlight::Write(write));
                        return Poll::Pending;
                    };
                    return Poll::Ready(joined.map_err(io::Error::other)?);
                }
                // A read given up on: let it finish before writing. What it read is lost,
                // as it would be had the read been cancelled any other way.
                Some(InFlight::Read(mut read)) => {
                    if std::pin::Pin::new(&mut read).poll(cx).is_pending() {
                        self.in_flight = Some(InFlight::Read(read));
                        return Poll::Pending;
                    }
                }
                None => (),
            }
            let tried = match wait {
                Wait::Readiness(_) => try_write(fd, buf)?,
                Wait::BlockingPool => try_write_fifo(fd, buf)?,
            };
            if let Some(count) = tried {
                return Poll::Ready(Ok(count));
            }
            if !can_wait() {
                return Poll::Ready(without_holding_up_the_runtime(|| {
                    Ok(rustix::io::write(fd, buf)?)
                }));
            }
            match wait {
                Wait::Readiness(interest) => {
                    let Some(registration) = self.registration(fd, interest)? else {
                        return Poll::Ready(write_fd(fd, buf));
                    };
                    ready!(registration.poll_write_ready(cx))?.clear_ready();
                }
                Wait::BlockingPool => {
                    let owned = fd.try_clone_to_owned()?;
                    let data = buf[..buf.len().min(MAX_FIFO_WRITE)].to_vec();
                    self.in_flight =
                        Some(InFlight::Write(tokio::task::spawn_blocking(move || {
                            Ok(retrying_interrupts(|| rustix::io::write(&owned, &data))?)
                        })));
                }
            }
        }
    }

    /// Returns the pipe's registration with the runtime, registering it first if need
    /// be, or `None` if it can't be registered. A registration the runtime refused
    /// isn't tried again.
    fn registration(
        &mut self,
        fd: BorrowedFd<'_>,
        interest: Interest,
    ) -> io::Result<Option<&AsyncFd<OwnedFd>>> {
        if matches!(self.registration, Registration::None) {
            let owned = fd.try_clone_to_owned()?;
            // SAFETY: `owned` is a descriptor of the registration's own, open until the
            // registration drops it, and an `OwnedFd` always returns the same one.
            self.registration = match unsafe { AsyncFd::register_with_interest(owned, interest) } {
                Ok(registration) => Registration::Registered(registration),
                Err(_) => Registration::Refused,
            };
        }
        Ok(match &self.registration {
            Registration::Registered(registration) => Some(registration),
            Registration::None | Registration::Refused => None,
        })
    }
}

/// Returns whether the current task can wait through the runtime, rather than block: it
/// runs on one, and not on a thread of its own.
fn can_wait() -> bool {
    !crate::tasks::runs_on_a_thread_of_its_own() && tokio::runtime::Handle::try_current().is_ok()
}

/// Runs `op` until it isn't interrupted by a signal.
fn retrying_interrupts(
    mut op: impl FnMut() -> rustix::io::Result<usize>,
) -> rustix::io::Result<usize> {
    loop {
        match op() {
            Err(rustix::io::Errno::INTR) => (),
            other => return other,
        }
    }
}

/// Which way an [`AsyncIo`] read or write goes.
#[derive(Clone, Copy)]
enum Direction {
    Read,
    Write,
}

/// How an [`AsyncIo`] read or write that would block waits.
#[derive(Clone, Copy)]
enum Wait {
    /// For the runtime to say the descriptor is ready for this.
    Readiness(Interest),
    /// On the blocking pool.
    BlockingPool,
}

/// Returns `file`'s descriptor if it's a pipe's end or a FIFO, which are waited for, with
/// how to wait for it. A read from a pipe's write end, or a write to its read end, isn't
/// waited for: it fails right away, as through the
/// [`OpenFile`](crate::openfiles::OpenFile) itself. (On a FIFO, the read or write
/// itself fails right away.)
///
/// A pipe's end is registered for its own direction only: some kernels refuse to watch
/// one for the other.
fn pipe_fd(
    file: &crate::openfiles::OpenFile,
    direction: Direction,
) -> Option<(BorrowedFd<'_>, Wait)> {
    use crate::openfiles::OpenFile;

    match (file, direction) {
        (OpenFile::PipeReader(reader), Direction::Read) => {
            Some((reader.as_fd(), Wait::Readiness(Interest::READABLE)))
        }
        (OpenFile::PipeWriter(writer), Direction::Write) => {
            Some((writer.as_fd(), Wait::Readiness(Interest::WRITABLE)))
        }
        (OpenFile::Fifo(fifo), _) => Some((fifo.as_fd(), Wait::BlockingPool)),
        _ => None,
    }
}

/// Runs `f`, which may block, so that the runtime's other tasks can still run: on a
/// runtime with several workers, this one hands its tasks to another thread first. On
/// a runtime with a single thread there's nowhere to hand them to, so `f` just runs.
///
/// This uses [`tokio::task::block_in_place`], which is only called on a multi-threaded
/// runtime, and which panics if the task can't hand its work to another worker thread.
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
    if let Some(result) = nowait::attempt(&nowait::PIPES, || {
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
    if let Some(result) = nowait::attempt(&nowait::PIPES, || {
        let bufs = &[io::IoSlice::new(buf)];
        rustix::io::pwritev2(fd, bufs, nowait::CURRENT_OFFSET, nowait::FLAGS)
    }) {
        return result;
    }

    try_write_ready(fd, buf)
}

/// [`try_write`] for a FIFO: only where the kernel can be told not to block. A FIFO that
/// polls writable may not take `PIPE_BUF` bytes without blocking, as a pipe would: on
/// macOS, a full one polls writable with less room than that.
#[cfg_attr(
    not(target_os = "linux"),
    expect(
        clippy::missing_const_for_fn,
        clippy::unnecessary_wraps,
        reason = "only Linux can tell a write not to block"
    )
)]
fn try_write_fifo(fd: BorrowedFd<'_>, buf: &[u8]) -> io::Result<Option<usize>> {
    #[cfg(target_os = "linux")]
    if let Some(result) = nowait::attempt(&nowait::FIFOS, || {
        let bufs = &[io::IoSlice::new(buf)];
        rustix::io::pwritev2(fd, bufs, nowait::CURRENT_OFFSET, nowait::FLAGS)
    }) {
        return result;
    }

    let _ = (fd, buf);
    Ok(None)
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

    /// Cleared once the kernel turns out not to take `RWF_NOWAIT` on anonymous pipes.
    pub(super) static PIPES: AtomicBool = AtomicBool::new(true);

    /// Cleared once the kernel turns out not to take `RWF_NOWAIT` on FIFOs. Kept apart
    /// from [`PIPES`]: a kernel may take it on one and refuse it on the other (Linux
    /// 7.0 does), and a refusal on a FIFO must not slow every pipe down.
    pub(super) static FIFOS: AtomicBool = AtomicBool::new(true);

    /// Runs `op`, returning how much it read or wrote, `None` inside if it would have
    /// blocked, or `None` if the kernel can't tell it not to block.
    pub(super) fn attempt(
        supported: &AtomicBool,
        op: impl FnOnce() -> rustix::io::Result<usize>,
    ) -> Option<std::io::Result<Option<usize>>> {
        if !supported.load(Ordering::Relaxed) {
            return None;
        }
        match op() {
            Ok(count) => Some(Ok(Some(count))),
            Err(Errno::AGAIN) => Some(Ok(None)),
            // An older kernel, one without `RWF_NOWAIT` for pipes, or a seccomp filter
            // that turns the call away.
            Err(Errno::OPNOTSUPP | Errno::INVAL | Errno::NOSYS | Errno::PERM) => {
                supported.store(false, Ordering::Relaxed);
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

    /// A FIFO, opened however, comes back as a FIFO, and anything else as a file.
    #[test]
    fn a_fifo_opens_as_a_fifo() -> io::Result<()> {
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

        for opened in [rw, r, w] {
            assert!(matches!(opened, OpenFile::Fifo(_)));
        }
        assert!(matches!(file, OpenFile::File(_)));
        Ok(())
    }

    /// A kernel that won't take a non-blocking write on a FIFO says nothing about pipes.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_refusal_on_a_fifo_leaves_pipes_alone() -> io::Result<()> {
        use std::sync::atomic::Ordering;

        let dir = tempfile::tempdir()?;
        let path = dir.path().join("p");
        nix::unistd::mkfifo(&path, nix::sys::stat::Mode::S_IRWXU)?;
        let fifo = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)?;

        let pipes_before = nowait::PIPES.load(Ordering::Relaxed);
        // Whether the kernel takes it or not, only the FIFOs' flag may change.
        let _ = try_write_fifo(fifo.as_fd(), b"x")?;
        assert_eq!(nowait::PIPES.load(Ordering::Relaxed), pipes_before);
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
