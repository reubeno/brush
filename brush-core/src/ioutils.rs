//! Internal I/O utilities.

use crate::openfiles;

/// Copies input to output, yielding when a cooperative input stream is not ready.
///
/// Reads and writes are synchronous. Run this on a blocking task if either stream
/// can block the executor's thread.
///
/// Unlike a native blocking pipe, an empty WASM pipe returns `WouldBlock` so its
/// producer can run on the same thread. Keep bytes already copied and retry the
/// read after yielding. Output errors are returned, not retried: a failed write
/// may already have consumed part of the current chunk.
pub(crate) async fn cooperative_copy(
    mut input: impl std::io::Read,
    mut output: impl std::io::Write,
) -> std::io::Result<()> {
    // 8 KiB is a heuristic chunk size to bound scratch space, not a pipe capacity
    // or correctness requirement; it has not been tuned with benchmarks.
    let mut buffer = [0; 8192];
    loop {
        match input.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(n) => output.write_all(&buffer[..n])?,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                // TODO: Wait for input readiness instead; yielding can busy-wait
                // when the input remains unavailable.
                tokio::task::yield_now().await;
            }
            Err(err) => return Err(err),
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Read, Write};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[tokio::test]
    async fn cooperative_copy_waits_for_input() -> anyhow::Result<()> {
        struct DelayedInput {
            ready: Arc<AtomicBool>,
            interrupted: bool,
            first: &'static [u8],
            rest: &'static [u8],
        }

        impl Read for DelayedInput {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                if !self.interrupted {
                    self.interrupted = true;
                    return Err(io::ErrorKind::Interrupted.into());
                }
                if !self.first.is_empty() {
                    return self.first.read(buffer);
                }
                if !self.ready.load(Ordering::SeqCst) {
                    return Err(io::ErrorKind::WouldBlock.into());
                }
                self.rest.read(buffer)
            }
        }

        let ready = Arc::new(AtomicBool::new(false));
        let input = DelayedInput {
            ready: ready.clone(),
            interrupted: false,
            first: b"first\n",
            rest: b"second\n",
        };
        let producer = tokio::spawn(async move { ready.store(true, Ordering::SeqCst) });
        let mut output = Vec::new();
        cooperative_copy(input, &mut output).await?;
        producer.await?;
        anyhow::ensure!(
            output == b"first\nsecond\n",
            "unexpected output: {output:?}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn cooperative_copy_does_not_retry_a_partially_failed_write() -> anyhow::Result<()> {
        #[derive(Default)]
        struct PartialWriter(Vec<u8>);

        impl Write for PartialWriter {
            fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
                if !self.0.is_empty() {
                    return Err(io::ErrorKind::WouldBlock.into());
                }
                let count = buffer.len().min(2);
                self.0.extend_from_slice(&buffer[..count]);
                Ok(count)
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let mut output = PartialWriter::default();
        let result = cooperative_copy(&b"abcdef"[..], &mut output).await;
        anyhow::ensure!(
            result
                .as_ref()
                .is_err_and(|err| err.kind() == io::ErrorKind::WouldBlock),
            "unexpected result: {result:?}"
        );
        anyhow::ensure!(output.0 == b"ab", "unexpected output: {:?}", output.0);
        Ok(())
    }
}
