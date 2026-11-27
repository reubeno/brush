//! Reads and writes on pipes, where there's no telling them not to block.

use std::io::{self, Read as _, Write as _};
use std::task::{Context, Poll};

/// Reads from `reader` into `buf`, as [`std::io::Read::read`] does.
pub(crate) fn read(reader: &std::io::PipeReader, buf: &mut [u8]) -> io::Result<usize> {
    let mut reader = reader;
    reader.read(buf)
}

/// Writes `buf` to `writer`, as [`std::io::Write::write`] does.
pub(crate) fn write(writer: &std::io::PipeWriter, buf: &[u8]) -> io::Result<usize> {
    let mut writer = writer;
    writer.write(buf)
}

/// Opens `path` as a redirection target, for an async caller.
#[allow(
    clippy::unused_async,
    reason = "matches the platforms that open FIFOs off-thread"
)]
pub(crate) async fn open_file_async(
    options: &std::fs::OpenOptions,
    path: &std::path::Path,
) -> io::Result<crate::openfiles::OpenFile> {
    open_file(options, path)
}

/// Opens `path` as a redirection target.
pub(crate) fn open_file(
    options: &std::fs::OpenOptions,
    path: &std::path::Path,
) -> io::Result<crate::openfiles::OpenFile> {
    Ok(options.open(path)?.into())
}

/// Reads and writes on an [`OpenFile`](crate::openfiles::OpenFile), done synchronously
/// where there's no waiting for a pipe to be ready.
#[derive(Default)]
pub(crate) struct AsyncIo(());

impl AsyncIo {
    /// Reads from `file` into `buf`.
    #[allow(
        clippy::unused_self,
        reason = "matches the platforms that wait for readiness"
    )]
    pub(crate) fn poll_read(
        &mut self,
        _cx: &mut Context<'_>,
        file: &mut crate::openfiles::OpenFile,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(file.read(buf))
    }

    /// Writes `buf` to `file`.
    #[allow(
        clippy::unused_self,
        reason = "matches the platforms that wait for readiness"
    )]
    pub(crate) fn poll_write(
        &mut self,
        _cx: &mut Context<'_>,
        file: &mut crate::openfiles::OpenFile,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(file.write(buf))
    }
}

/// Reads from a FIFO open for reading and writing into `buf`.
pub(crate) fn read_fifo(fifo: &std::fs::File, buf: &mut [u8]) -> io::Result<usize> {
    let mut fifo = fifo;
    fifo.read(buf)
}

/// Writes `buf` to a FIFO open for reading and writing.
pub(crate) fn write_fifo(fifo: &std::fs::File, buf: &[u8]) -> io::Result<usize> {
    let mut fifo = fifo;
    fifo.write(buf)
}

/// Returns whether `path` names a FIFO; there are none here.
pub(crate) const fn is_fifo(_path: &std::path::Path) -> bool {
    false
}
