//! Reads and writes on pipes, where there's no telling them not to block.

use std::io::{self, Read as _, Write as _};

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

/// Opens `path` as a redirection target.
pub(crate) fn open_file(
    options: &std::fs::OpenOptions,
    path: &std::path::Path,
) -> io::Result<crate::openfiles::OpenFile> {
    Ok(options.open(path)?.into())
}

/// Returns whether `path` names a FIFO; there are none here.
pub(crate) const fn is_fifo(_path: &std::path::Path) -> bool {
    false
}
