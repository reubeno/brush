//! Internal I/O utilities.

use crate::openfiles;

/// Error type for `FailingReaderWriter`.
#[derive(Clone, Debug, thiserror::Error)]
enum ReaderWriterError {
    /// I/O read error.
    #[error("I/O read error: {0}")]
    Read(&'static str),
    /// I/O write error.
    #[error("I/O write error: {0}")]
    Write(&'static str),
    /// I/O flush error.
    #[error("I/O flush error: {0}")]
    Flush(&'static str),
}

/// An implementation of `std::io::Read` and `std::io::Write` that always fails.
///
/// Also implements `openfiles::Stream` for use with `openfiles::OpenFile`. Useful
/// for providing a valid stream that gracefully fails all operations in cases where
/// no real I/O is possible.
#[derive(Clone)]
pub(crate) struct FailingReaderWriter {
    message: &'static str,
}

impl FailingReaderWriter {
    /// Creates a new `FailingReaderWriter` with the given error message.
    ///
    /// # Arguments
    ///
    /// * `message` - The error message to use for all operations.
    pub const fn new(message: &'static str) -> Self {
        Self { message }
    }
}

impl openfiles::Stream for FailingReaderWriter {
    fn clone_box(&self) -> Box<dyn openfiles::Stream> {
        Box::new(self.clone())
    }

    #[cfg(unix)] // ast-grep-ignore: platform-cfg-outside-sys
    fn try_clone_to_owned(&self) -> Result<std::os::fd::OwnedFd, crate::error::Error> {
        Err(crate::error::ErrorKind::CannotConvertToNativeFd.into())
    }

    #[cfg(unix)] // ast-grep-ignore: platform-cfg-outside-sys
    fn try_borrow_as_fd(&self) -> Result<std::os::fd::BorrowedFd<'_>, crate::error::Error> {
        Err(crate::error::ErrorKind::CannotConvertToNativeFd.into())
    }
}

impl std::io::Read for FailingReaderWriter {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other(ReaderWriterError::Read(self.message)))
    }
}

impl std::io::Write for FailingReaderWriter {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other(ReaderWriterError::Write(
            self.message,
        )))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::other(ReaderWriterError::Flush(
            self.message,
        )))
    }
}

impl From<FailingReaderWriter> for openfiles::OpenFile {
    fn from(frw: FailingReaderWriter) -> Self {
        Self::Stream(Box::new(frw))
    }
}

/// What a write to one of a command's standard streams failed with, in bash's
/// words: `write error: No space left on device`.
#[derive(Debug)]
struct WriteError(std::io::Error);

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "write error: {}",
            crate::error::io_error_message(&self.0)
        )
    }
}

impl std::error::Error for WriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

/// A writer whose errors say that a write failed (`write error: …`). The error
/// keeps its kind, so a broken pipe is still one.
pub(crate) struct WriteErrorTagging<W>(W);

impl<W: std::io::Write> WriteErrorTagging<W> {
    /// Wraps `writer`.
    pub const fn new(writer: W) -> Self {
        Self(writer)
    }
}

/// Tags `error` as a write error, unless it already is one.
fn tag_write_error(error: std::io::Error) -> std::io::Error {
    if error
        .get_ref()
        .is_some_and(|inner| inner.is::<WriteError>())
    {
        return error;
    }
    std::io::Error::new(error.kind(), WriteError(error))
}

impl<W: std::io::Write> std::io::Write for WriteErrorTagging<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf).map_err(tag_write_error)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush().map_err(tag_write_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[test]
    fn a_tagged_write_error_keeps_its_kind_and_says_what_failed() {
        let mut full = WriteErrorTagging::new(FailingReaderWriter::new("disk full"));
        let error = full.write_all(b"x").unwrap_err();
        assert_eq!(error.to_string(), "write error: I/O write error: disk full");
        assert!(
            error
                .get_ref()
                .is_some_and(|inner| inner.is::<WriteError>())
        );
        // Tagging twice is tagging once.
        let again = tag_write_error(error);
        assert_eq!(again.to_string(), "write error: I/O write error: disk full");

        let broken = tag_write_error(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
        assert_eq!(broken.kind(), std::io::ErrorKind::BrokenPipe);
    }
}
